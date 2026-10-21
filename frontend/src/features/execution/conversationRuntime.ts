import type { AgentEvent } from "../../api/chat";
import type { Conversation, Message, WorkspaceMode } from "../../types";
import type { PendingApproval } from "../../types/approval";
import type { ChatRequestOptions } from "../../components/chat/chatRequestOptions";
import { createInitialAgentRunState, createInitialAgentWorkspaceState, reduceAgentEvent, reduceExecutionWorkspace, selectActiveRun, toToolCallRecords } from "./reducer";
import { createDecisionGate } from "./decisionGate";
import { reconcilePersistedApproval } from "./approvalReconciliation";
import type { AgentRunState } from "./model";
import { projectConversationMessages } from "./conversationProcess";

interface RuntimeDependencies {
  loadConversation: (id: string) => Promise<Conversation>;
  listPendingApprovals: () => Promise<PendingApproval[] | null>;
  getApproval: (id: string) => Promise<PendingApproval | null>;
  sendMessage: (text: string, id: string | null, onEvent: (event: AgentEvent) => void, workflowId?: string | null, options?: ChatRequestOptions) => AbortController;
  approveAction: (id: string, conversationId: string | null, onEvent: (event: AgentEvent) => void) => Promise<void>;
  rejectAction: (id: string, conversationId: string | null, onEvent: (event: AgentEvent) => void) => Promise<void>;
  stopGeneration: (id: string) => Promise<unknown>;
  setWorkspaceMode: (id: string, mode: WorkspaceMode) => Promise<{ mode: WorkspaceMode }>;
  onApprovals?: (id: string, approvals: PendingApproval[]) => void;
}
export interface ConversationRuntimeSnapshot {
  messages: Message[];
  run: AgentRunState;
  loading: boolean;
  error: string | null;
  finished: boolean;
  lastSubmittedText: string;
  modelRoute: AgentEvent | null;
  conversation: Conversation | null;
  pendingApprovals: PendingApproval[];
  resolving: Record<string, boolean>;
  workspaceMode: WorkspaceMode;
  modeChanging: boolean;
  modeError: string | null;
  modeNotice: { mode: WorkspaceMode; reason: string; automatic: boolean } | null;
  live: boolean;
}
interface RuntimeEntry {
  snapshot: ConversationRuntimeSnapshot;
  listeners: Set<() => void>;
  revision: number;
  refreshVersion: number;
  streaming: boolean;
  streamError: string | null;
  submittedAt: number | null;
  previousRunStartedAt: number | null;
  accepted: boolean;
  requestRejected: boolean;
  poll: ReturnType<typeof setTimeout> | null;
}

/** Conversation transports and execution outlive every page that observes them. */
export function createConversationRuntime(dependencies: RuntimeDependencies) {
  const entries = new Map<string, RuntimeEntry>();
  function entry(id: string): RuntimeEntry {
    let value = entries.get(id);
    if (!value) {
      value = {
        snapshot: { messages: [], run: createInitialAgentRunState(id), loading: false, error: null,
          finished: false, lastSubmittedText: "", modelRoute: null, conversation: null, pendingApprovals: [], resolving: {},
          workspaceMode: "chat", modeChanging: false, modeError: null, modeNotice: null, live: false },
        listeners: new Set(), revision: 0, refreshVersion: 0, streaming: false, streamError: null,
        submittedAt: null, previousRunStartedAt: null, accepted: false, requestRejected: false, poll: null,
      };
      entries.set(id, value);
    }
    return value;
  }
  function publish(id: string, patch: Partial<ConversationRuntimeSnapshot>) {
    const value = entry(id);
    value.snapshot = { ...value.snapshot, ...patch };
    if (patch.pendingApprovals) dependencies.onApprovals?.(id, patch.pendingApprovals);
    value.listeners.forEach((listener) => listener());
  }
  function schedule(id: string) {
    const value = entry(id);
    if (value.poll) clearTimeout(value.poll);
    value.poll = null;
    const status = value.snapshot.conversation?.run_status;
    if (!value.streaming && (status === "running" || status === "waiting_approval")) {
      value.poll = setTimeout(() => { value.poll = null; void refresh(id); }, 1500);
    }
  }
  async function refresh(id: string) {
    const value = entry(id);
    if (value.streaming) return;
    const revision = value.revision;
    const version = ++value.refreshVersion;
    try {
      const [conversation, approvals] = await Promise.all([
        dependencies.loadConversation(id), dependencies.listPendingApprovals(),
      ]);
      if (value.streaming || revision !== value.revision || version !== value.refreshVersion) return;
      const workspace = reduceExecutionWorkspace(createInitialAgentWorkspaceState(id), {
        type: "hydrate_history", conversationId: id,
        toolCalls: conversation.messages.flatMap((message) => message.tool_calls || []),
        executionHistory: conversation.execution_history,
      });
      let run = selectActiveRun(workspace);
      const pendingApprovals = approvals === null ? [...value.snapshot.pendingApprovals] : approvals.filter((approval) => approval.conversation_id === id);
      const recordsToReconcile = run.order.map((recordId) => run.records[recordId]).filter((record) =>
        record.approvalStatus === "pending" && record.approvalId && !pendingApprovals.some((approval) => approval.approval_id === record.approvalId));
      const resolved = await Promise.all(recordsToReconcile.map(async (record) => ({ record, approval: await dependencies.getApproval(record.approvalId!) })));
      if (value.streaming || revision !== value.revision || version !== value.refreshVersion) return;
      for (const { record, approval } of resolved) {
        const reconciliation = reconcilePersistedApproval(record, approval);
        if (reconciliation.kind === "pending") pendingApprovals.push(reconciliation.approval);
        if (reconciliation.kind === "resolved") run = reduceAgentEvent(run, reconciliation.event);
      }
      // Pending approvals are authoritative even if the history write is still in flight.
      for (const approval of pendingApprovals) {
        run = reduceAgentEvent(run, { type: "approval_required", conversation_id: id,
          tool_call_id: approval.tool_call_id, tool_name: approval.tool_name, args: approval.arguments,
          approval_id: approval.approval_id, risk_level: approval.risk_level, reason: approval.reason });
      }
      const status = conversation.run_status;
      const failed = status === "failed" || status === "interrupted";
      const terminal = status === "completed" || failed || status === "cancelled";
      const persistedCurrentRun = value.submittedAt !== null && conversation.run_started_at != null
        && conversation.run_started_at >= value.submittedAt && conversation.run_started_at !== value.previousRunStartedAt;
      if (status === "completed" && !value.requestRejected && (value.accepted || persistedCurrentRun)) value.streamError = null;
      run = { ...run, connection: failed ? "error" : status === "idle" ? "idle" : "connected",
        terminal, terminalError: failed ? conversation.run_error || "执行已中断" : null };
      publish(id, { conversation, run, messages: projectConversationMessages(conversation), live: false,
        workspaceMode: conversation.workspace_mode ?? value.snapshot.workspaceMode,
        loading: status === "running", error: run.terminalError || value.streamError, pendingApprovals,
        lastSubmittedText: [...conversation.messages].reverse().find((message) => message.role === "user")?.content || value.snapshot.lastSubmittedText,
        finished: status === "completed" && !value.streamError, resolving: {} });
    } catch (error) {
      if (revision === value.revision && version === value.refreshVersion) {
        publish(id, { error: error instanceof Error ? error.message : "读取执行状态失败" });
      }
    }
    schedule(id);
  }
  function event(id: string, incoming: AgentEvent) {
    if (incoming.conversation_id && incoming.conversation_id !== id) return;
    const value = entry(id);
    const previous = value.snapshot;
    if (["connected", "token", "reasoning", "reasoning_snapshot", "tool_start", "tool_end", "approval_required", "approval_resolved", "done"].includes(incoming.type)) value.accepted = true;
    if (incoming.request_rejected) value.requestRejected = true;
    const run = reduceAgentEvent(previous.run, incoming);
    const patch: Partial<ConversationRuntimeSnapshot> = { run };
    if (incoming.type === "workspace_mode" && incoming.mode) {
      patch.workspaceMode = incoming.mode;
      patch.modeNotice = { mode: incoming.mode, reason: incoming.reason || "", automatic: incoming.automatic === true };
    }
    if (incoming.type === "approval_required" && incoming.approval_id) {
      const risk = incoming.risk_level;
      const approval: PendingApproval = { approval_id: incoming.approval_id, conversation_id: id,
        tool_call_id: incoming.tool_call_id || "", tool_name: incoming.tool_name || "", arguments: incoming.args || {},
        risk_level: risk === "low" || risk === "medium" || risk === "high" || risk === "critical" ? risk : "unknown",
        reason: incoming.reason || "", status: "pending", created_at: new Date().toISOString(), expires_at: "" };
      patch.pendingApprovals = [...previous.pendingApprovals.filter((item) => item.approval_id !== approval.approval_id), approval];
    }
    if (incoming.type === "approval_resolved") {
      patch.pendingApprovals = previous.pendingApprovals.filter((approval) => approval.approval_id !== incoming.approval_id);
    }
    if (incoming.type === "model_route") {
      const before = previous.modelRoute?.interrupted_content;
      const next = incoming.interrupted_content;
      patch.modelRoute = { ...incoming, interrupted_content: before && next && before !== next ? `${before}\n\n────────\n\n${next}` : next || before };
    }
    if (incoming.type === "done") {
      const message: Message = { id: incoming.message_id || crypto.randomUUID(), role: "assistant", content: run.content,
        created_at: Date.now(), process: { reasoning: run.reasoning || undefined, tools: toToolCallRecords(run).map(record => ({ ...record, execution: record })) } };
      patch.messages = previous.messages.some((item) => item.id === message.id) ? previous.messages : [...previous.messages, message];
      patch.loading = false;
      patch.finished = true;
    }
    if (incoming.type === "error") { value.streamError = incoming.error || "生成失败"; patch.loading = false; patch.error = value.streamError; patch.finished = false; }
    if (incoming.type === "stream_end") {
      value.streaming = false;
      patch.resolving = {};
      patch.live = false;
      // EOF is not a backend terminal state. Read the persisted run state.
      if (previous.pendingApprovals.length > 0) patch.loading = false;
    }
    publish(id, patch);
    if (incoming.type === "stream_end") void refresh(id);
  }
  function start(id: string) {
    const value = entry(id);
    value.revision++;
    value.streaming = true;
    value.streamError = null;
    value.submittedAt = Date.now();
    value.previousRunStartedAt = value.snapshot.conversation?.run_started_at ?? null;
    value.accepted = false;
    value.requestRejected = false;
    if (value.poll) clearTimeout(value.poll);
    value.poll = null;
    publish(id, { loading: true, error: null, finished: false, live: true });
    return value.revision;
  }
  const decisions = createDecisionGate(async (approvalId, decision) => {
    const match = [...entries.entries()].find(([, value]) => value.snapshot.pendingApprovals.some((approval) => approval.approval_id === approvalId));
    if (!match) throw new Error("审批已不在待确认列表，请刷新执行状态");
    const [id, value] = match;
    const revision = start(id);
    publish(id, { resolving: { ...value.snapshot.resolving, [approvalId]: true } });
    try {
      const submit = decision === "approve" ? dependencies.approveAction : dependencies.rejectAction;
      await submit(approvalId, id, (incoming) => { if (revision === value.revision) event(id, incoming); });
      if (revision === value.revision) event(id, { type: "stream_end", conversation_id: id });
    } catch (error) {
      value.streaming = false;
      publish(id, { loading: false, resolving: {}, error: error instanceof Error ? error.message : "审批请求失败" });
      // Another client may already have consumed this approval and resumed the task.
      // Reconcile its actual status and restore observation before returning the error.
      await refresh(id);
      throw error;
    }
  });
  return {
    snapshot: (id: string) => entry(id).snapshot,
    subscribe: (id: string, listener: () => void) => { entry(id).listeners.add(listener); return () => { entry(id).listeners.delete(listener); }; },
    refresh,
    async changeWorkspaceMode(id: string, mode: WorkspaceMode) {
      const value = entry(id);
      publish(id, { modeChanging: true, modeError: null });
      try {
        const result = id === "__draft__" ? { mode } : await dependencies.setWorkspaceMode(id, mode);
        value.refreshVersion++;
        publish(id, { workspaceMode: result.mode, modeNotice: null });
      } catch (error) {
        publish(id, { modeError: error instanceof Error ? error.message : "切换模式失败" });
        throw error;
      } finally { publish(id, { modeChanging: false }); }
    },
    send(id: string, text: string, workflowId?: string | null, options?: ChatRequestOptions) {
      const value = entry(id);
      if (value.snapshot.loading || value.snapshot.pendingApprovals.length > 0) return false;
      const revision = start(id);
      const requestOptions = { ...options, workspace_mode: options?.workspace_mode ?? value.snapshot.workspaceMode };
      publish(id, { run: { ...createInitialAgentRunState(id), connection: "connecting" }, modelRoute: null,
        workspaceMode: requestOptions.workspace_mode,
        lastSubmittedText: text, messages: [...value.snapshot.messages, { id: crypto.randomUUID(), role: "user", content: text, created_at: Date.now() }] });
      dependencies.sendMessage(text, id, (incoming) => { if (revision === value.revision) event(id, incoming); }, workflowId, requestOptions);
      return true;
    },
    decide(approval: PendingApproval, decision: "approve" | "reject") { return decisions.submit(approval.approval_id, decision); },
    async stop(id: string) {
      await dependencies.stopGeneration(id);
      // Keep listening until the backend reports and persists cancellation.
      if (!entry(id).streaming) await refresh(id);
    },
  };
}
