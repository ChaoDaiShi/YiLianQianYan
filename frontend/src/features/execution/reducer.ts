import type { AgentEvent } from "../../api/client";
import type { ToolCallRecord } from "../../types";
import type {
  AgentRunState,
  AgentWorkspaceState,
  ApprovalStatus,
  ExecutionRecord,
  ExecutionRiskLevel,
  ExecutionStatus,
  VerificationStatus,
} from "./model";

const DRAFT_KEY = "__draft__";
const KNOWN_EVENT_TYPES = new Set([
  "connected",
  "token",
  "tool_start",
  "tool_end",
  "approval_required",
  "approval_resolved",
  "verification",
  "done",
  "error",
  "stream_end",
]);

export type ExecutionAction =
  | { type: "activate_conversation"; conversationId: string | null }
  | { type: "start_run"; conversationId: string | null }
  | { type: "agent_event"; event: AgentEvent }
  | {
      type: "hydrate_history";
      conversationId: string;
      toolCalls: unknown;
      executionHistory?: unknown;
    }
  | { type: "consume_completed" };

function runKey(conversationId: string | null | undefined): string {
  return conversationId || DRAFT_KEY;
}

function asRiskLevel(value: unknown): ExecutionRiskLevel {
  return value === "low" ||
    value === "medium" ||
    value === "high" ||
    value === "critical"
    ? value
    : "unknown";
}

function asApprovalStatus(value: unknown): ApprovalStatus {
  return value === "pending" ||
    value === "approved" ||
    value === "rejected" ||
    value === "cancelled" ||
    value === "expired" ||
    value === "not_required"
    ? value
    : "not_required";
}

function asVerificationStatus(value: unknown): VerificationStatus {
  return value === "pending" || value === "passed" || value === "failed"
    ? value
    : "not_requested";
}

function executionFromLegacyStatus(value: unknown): ExecutionStatus {
  switch (value) {
    case "running":
      return "running";
    case "success":
      return "succeeded";
    case "error":
      return "failed";
    case "blocked":
      return "awaiting_approval";
    default:
      return "queued";
  }
}

function asExecutionStatus(value: unknown): ExecutionStatus {
  return value === "queued" ||
    value === "awaiting_approval" ||
    value === "running" ||
    value === "interrupted" ||
    value === "succeeded" ||
    value === "failed" ||
    value === "rejected" ||
    value === "cancelled"
    ? value
    : "queued";
}

function persistedStatus(value: ExecutionStatus): ToolCallRecord["status"] {
  switch (value) {
    case "running":
    case "queued":
      return "running";
    case "succeeded":
      return "success";
    case "awaiting_approval":
      return "blocked";
    case "failed":
    case "interrupted":
    case "rejected":
    case "cancelled":
      return "error";
  }
}

function findRecordId(state: AgentRunState, event: AgentEvent): string | null {
  if (event.tool_call_id) return event.tool_call_id;
  if (!event.approval_id) return null;
  return (
    state.order.find(
      (recordId) => state.records[recordId]?.approvalId === event.approval_id
    ) || null
  );
}

function upsertRecord(
  state: AgentRunState,
  toolCallId: string,
  event: AgentEvent,
  patch: Partial<ExecutionRecord>
): AgentRunState {
  const existing = state.records[toolCallId];
  const sequence = existing?.sequence ?? state.nextSequence;
  const base: ExecutionRecord = existing || {
    toolCallId,
    conversationId: event.conversation_id || state.conversationId || "",
    name: "unknown_tool",
    args: {},
    riskLevel: "unknown",
    executionStatus: "queued",
    approvalStatus: "not_required",
    verificationStatus: "not_requested",
    sequence,
  };
  const record: ExecutionRecord = {
    ...base,
    conversationId: event.conversation_id || base.conversationId,
    name: event.tool_name || base.name,
    args: event.args || base.args,
    riskLevel: event.risk_level
      ? asRiskLevel(event.risk_level)
      : base.riskLevel,
    ...patch,
    toolCallId,
    sequence,
  };

  return {
    ...state,
    records: { ...state.records, [toolCallId]: record },
    order: existing ? state.order : [...state.order, toolCallId],
    nextSequence: existing ? state.nextSequence : state.nextSequence + 1,
    terminal: false,
    terminalError: null,
  };
}

export function createInitialAgentRunState(
  conversationId: string | null = null
): AgentRunState {
  return {
    conversationId,
    content: "",
    records: {},
    order: [],
    nextSequence: 1,
    connection: "idle",
    terminalError: null,
    terminal: false,
  };
}

export function reduceAgentEvent(
  state: AgentRunState,
  event: AgentEvent
): AgentRunState {
  if (!KNOWN_EVENT_TYPES.has(event.type)) return state;

  switch (event.type) {
    case "connected":
      return {
        ...state,
        conversationId: event.conversation_id || state.conversationId,
        connection: "connected",
        terminalError: null,
        terminal: false,
      };
    case "token":
      return {
        ...state,
        content: state.content + (event.token || ""),
        connection: "connected",
        terminalError: null,
        terminal: false,
      };
    case "tool_start": {
      const toolCallId = findRecordId(state, event);
      if (!toolCallId) return state;
      return upsertRecord(state, toolCallId, event, {
        executionStatus: "running",
        startedAt: state.records[toolCallId]?.startedAt ?? Date.now(),
      });
    }
    case "approval_required": {
      const toolCallId = findRecordId(state, event);
      if (!toolCallId) return state;
      return upsertRecord(state, toolCallId, event, {
        approvalId: event.approval_id,
        approvalStatus: "pending",
        executionStatus: "awaiting_approval",
        reason: event.reason,
      });
    }
    case "approval_resolved": {
      const toolCallId = findRecordId(state, event);
      if (!toolCallId || !state.records[toolCallId]) return state;
      const approvalStatus = asApprovalStatus(event.status);
      const executionStatus: ExecutionStatus =
        approvalStatus === "rejected"
          ? "rejected"
          : approvalStatus === "cancelled" || approvalStatus === "expired"
            ? "cancelled"
            : "queued";
      return upsertRecord(state, toolCallId, event, {
        approvalStatus,
        executionStatus,
      });
    }
    case "tool_end": {
      const toolCallId = findRecordId(state, event);
      if (!toolCallId) return state;
      return upsertRecord(state, toolCallId, event, {
        executionStatus: event.status === "success" ? "succeeded" : "failed",
        result: event.result,
        finishedAt: Date.now(),
      });
    }
    case "verification": {
      const toolCallId = findRecordId(state, event);
      if (!toolCallId || !state.records[toolCallId]) return state;
      return upsertRecord(state, toolCallId, event, {
        verificationStatus:
          event.verification_success === true ? "passed" : "failed",
        verificationReason: event.verification_reason,
      });
    }
    case "done":
      return {
        ...state,
        connection: "connected",
        terminalError: null,
        terminal: true,
      };
    case "error":
      return {
        ...state,
        connection: "error",
        terminalError: event.error || "生成失败",
        terminal: true,
      };
    case "stream_end": {
      const hasPendingApproval = state.order.some(
        (recordId) => state.records[recordId]?.approvalStatus === "pending"
      );
      if (state.terminal || hasPendingApproval) return state;
      const records = Object.fromEntries(
        Object.entries(state.records).map(([recordId, record]) => [
          recordId,
          record.executionStatus === "running" ||
          record.executionStatus === "queued"
            ? { ...record, executionStatus: "interrupted" as const }
            : record,
        ])
      );
      return { ...state, records, connection: "interrupted" };
    }
    default:
      return state;
  }
}

export function hydrateToolCallRecords(raw: unknown): ExecutionRecord[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((value, index) => {
    if (!value || typeof value !== "object") return [];
    const source = value as Partial<ToolCallRecord> & Record<string, unknown>;
    const toolCallId =
      typeof source.toolCallId === "string" && source.toolCallId
        ? source.toolCallId
        : `legacy-${index + 1}`;
    const status = executionFromLegacyStatus(source.status);
    return [
      {
        toolCallId,
        conversationId: "",
        name: typeof source.name === "string" ? source.name : "unknown_tool",
        args:
          source.args && typeof source.args === "object" && !Array.isArray(source.args)
            ? (source.args as Record<string, unknown>)
            : {},
        riskLevel: asRiskLevel(source.riskLevel),
        executionStatus: status,
        approvalStatus:
          source.status === "blocked"
            ? "pending"
            : asApprovalStatus(source.approvalStatus),
        verificationStatus: asVerificationStatus(source.verificationStatus),
        verificationReason:
          typeof source.verificationReason === "string"
            ? source.verificationReason
            : undefined,
        result: typeof source.result === "string" ? source.result : undefined,
        startedAt:
          typeof source.startedAt === "number" ? source.startedAt : undefined,
        finishedAt:
          typeof source.finishedAt === "number" ? source.finishedAt : undefined,
        sequence: index + 1,
      },
    ];
  });
}

export function hydratePersistedExecutionRecords(
  raw: unknown,
  conversationId: string
): ExecutionRecord[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((value, index) => {
    if (!value || typeof value !== "object") return [];
    const source = value as Record<string, unknown>;
    if (typeof source.toolCallId !== "string" || !source.toolCallId) return [];
    return [
      {
        toolCallId: source.toolCallId,
        conversationId,
        approvalId:
          typeof source.approvalId === "string" ? source.approvalId : undefined,
        name: typeof source.name === "string" ? source.name : "unknown_tool",
        args:
          source.args && typeof source.args === "object" && !Array.isArray(source.args)
            ? (source.args as Record<string, unknown>)
            : {},
        riskLevel: asRiskLevel(source.riskLevel),
        reason: typeof source.reason === "string" ? source.reason : undefined,
        executionStatus: asExecutionStatus(source.executionStatus),
        approvalStatus: asApprovalStatus(source.approvalStatus),
        verificationStatus: asVerificationStatus(source.verificationStatus),
        verificationReason:
          typeof source.verificationReason === "string"
            ? source.verificationReason
            : undefined,
        result: typeof source.result === "string" ? source.result : undefined,
        startedAt:
          typeof source.startedAt === "number" ? source.startedAt : undefined,
        finishedAt:
          typeof source.finishedAt === "number" ? source.finishedAt : undefined,
        sequence:
          typeof source.sequence === "number" ? source.sequence : index + 1,
      },
    ];
  });
}

export function toToolCallRecords(
  input: AgentRunState | ExecutionRecord[]
): ToolCallRecord[] {
  const records = Array.isArray(input)
    ? input
    : input.order.map((recordId) => input.records[recordId]).filter(Boolean);
  return records.map((record) => ({
    toolCallId: record.toolCallId,
    name: record.name,
    args: record.args,
    status: persistedStatus(record.executionStatus),
    result: record.result,
    riskLevel: record.riskLevel,
    approvalStatus: record.approvalStatus,
    verificationStatus: record.verificationStatus,
    verificationReason: record.verificationReason,
    startedAt: record.startedAt,
    finishedAt: record.finishedAt,
  }));
}

export function createInitialAgentWorkspaceState(
  activeConversationId: string | null = null
): AgentWorkspaceState {
  const key = runKey(activeConversationId);
  return {
    activeConversationId,
    runs: { [key]: createInitialAgentRunState(activeConversationId) },
    completed: null,
  };
}

export function selectActiveRun(state: AgentWorkspaceState): AgentRunState {
  return (
    state.runs[runKey(state.activeConversationId)] ||
    createInitialAgentRunState(state.activeConversationId)
  );
}

export function reduceExecutionWorkspace(
  state: AgentWorkspaceState,
  action: ExecutionAction
): AgentWorkspaceState {
  switch (action.type) {
    case "start_run": {
      const key = runKey(action.conversationId);
      return {
        ...state,
        activeConversationId: action.conversationId,
        runs: {
          ...state.runs,
          [key]: {
            ...createInitialAgentRunState(action.conversationId),
            connection: "connecting",
          },
        },
        completed: null,
      };
    }
    case "activate_conversation": {
      const key = runKey(action.conversationId);
      if (state.activeConversationId === action.conversationId && state.runs[key]) {
        return state;
      }
      return {
        ...state,
        activeConversationId: action.conversationId,
        runs: state.runs[key]
          ? state.runs
          : {
              ...state.runs,
              [key]: createInitialAgentRunState(action.conversationId),
            },
      };
    }
    case "hydrate_history": {
      const hydratedById = new Map<string, ExecutionRecord>();
      const persistedRecords = hydratePersistedExecutionRecords(
        action.executionHistory,
        action.conversationId
      );
      persistedRecords.forEach((record) => {
        hydratedById.set(record.toolCallId, record);
      });
      const persistedIds = new Set(persistedRecords.map((record) => record.toolCallId));
      hydrateToolCallRecords(action.toolCalls).forEach((record) => {
        if (!persistedIds.has(record.toolCallId)) {
          hydratedById.set(record.toolCallId, {
            ...record,
            conversationId: action.conversationId,
          });
        }
      });
      const hydrated = [...hydratedById.values()];
      const key = runKey(action.conversationId);
      const current = state.runs[key] || createInitialAgentRunState(action.conversationId);
      return {
        ...state,
        runs: {
          ...state.runs,
          [key]: {
            ...current,
            records: Object.fromEntries(
              hydrated.map((record) => [record.toolCallId, record])
            ),
            order: hydrated.map((record) => record.toolCallId),
            nextSequence: hydrated.length + 1,
          },
        },
      };
    }
    case "consume_completed":
      return state.completed ? { ...state, completed: null } : state;
    case "agent_event": {
      const event = action.event;
      if (!KNOWN_EVENT_TYPES.has(event.type)) return state;

      if (event.type === "connected" && event.conversation_id) {
        const actualKey = runKey(event.conversation_id);
        const draft = state.runs[DRAFT_KEY];
        const base = state.runs[actualKey] || draft || createInitialAgentRunState(event.conversation_id);
        const nextRun = reduceAgentEvent(
          { ...base, conversationId: event.conversation_id },
          event
        );
        const runs = { ...state.runs, [actualKey]: nextRun };
        if (draft && actualKey !== DRAFT_KEY) delete runs[DRAFT_KEY];
        return {
          ...state,
          activeConversationId:
            state.activeConversationId === null
              ? event.conversation_id
              : state.activeConversationId,
          runs,
        };
      }

      const conversationId = event.conversation_id || state.activeConversationId;
      const key = runKey(conversationId);
      const current = state.runs[key] || createInitialAgentRunState(conversationId);
      const nextRun = reduceAgentEvent(current, event);
      if (nextRun === current) return state;

      const completed =
        event.type === "done" && conversationId
          ? {
              conversationId,
              messageId:
                event.message_id ||
                `${conversationId}-assistant-${nextRun.nextSequence}`,
              content: nextRun.content,
              records: nextRun.order
                .map((recordId) => nextRun.records[recordId])
                .filter(Boolean),
            }
          : state.completed;

      return {
        ...state,
        runs: { ...state.runs, [key]: nextRun },
        completed,
      };
    }
  }
}
