import { notifyConversationHistoryChanged } from "./conversationHistoryEvents";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useSyncExternalStore,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import {
  activateWorkflow,
  createConversation,
  listWorkflows,
  type Workflow,
} from "../../api/client";
import { ArrowDown, ShieldAlert, FolderOpen } from "lucide-react";
import { useLocation } from "react-router-dom";
import ConversationWorkspaceControl, { DefaultWorkspaceLink } from "./ConversationWorkspaceControl";
import ConversationGoalControl from "./ConversationGoalControl";
import ConversationOutputsModal from "../../features/outputs/ConversationOutputsModal";
import { bindResources, getResource, listResourceBindings, unbindResource } from "../../api/resources";
import { getChatModelReadiness } from "../../api/providerConnection";
import { sendChatTaskCommand, steerChat } from "../../api/chat";
import { createConversationLane } from "../../api/conversationLanes";
import type { ConversationalAnchor } from "../../api/voice";
import type { ChatVoiceControls } from "../../features/voice/ChatVoiceInput";
import { chatRuntime } from "../../features/execution/chatRuntime";
import { toToolCallRecords } from "../../features/execution/reducer";
import { workspaceModes, type ChatRequestOptions, type WorkspaceMode } from "./chatRequestOptions";
import type { PendingApproval } from "../../types/approval";
import type { Message } from "../../types";
import type { AgentRunState } from "../../features/execution/model";
import {
  CHAT_COLUMN_VIEWPORT_CLASS_NAME,
  isNearBottom,
  scrollMessageListToBottom,
} from "../layout/workspaceLayout";
import { createConversationOwnership } from "./conversationOwnership";
import ChatInput, { type RestoredComposerDraft } from "./ChatInput";
import UserInputCard from "./UserInputCard";
import ChatHeader from "./ChatHeader";
import MessageList, { type StreamingState } from "./MessageList";
import { createConversationBranch } from "../../api/conversations";
import WorkbenchHome from "./WorkbenchHome";
import ModelRouteNotice, { modelRouteReasonLabel } from "../../features/llm/ModelRouteNotice";
import { showToast } from "../ui";
import { ConversationLanes } from "./ConversationLanes";

interface ChatViewProps {
  conversationId: string | null;
  onConversationChange: (id: string | null) => void;
  showConversationToggle: boolean;
  showExecutionToggle: boolean;
  conversationToggleRef?: RefObject<HTMLButtonElement>;
  executionToggleRef?: RefObject<HTMLButtonElement>;
  onToggleConversations: () => void;
  onToggleExecution: () => void;
  onVoiceAnchorChange?: (anchor: ConversationalAnchor | null) => void;
  conversationRefreshRevision?: number;
  onOpenCurrentTask: () => void;
  renderExecution?: (controller: ExecutionController) => ReactNode;
  /** Provided by the existing GlobalVoiceHost through the application root. */
  chatVoiceControls?: ChatVoiceControls | null;
  onRequestGlobalVoiceSession?: () => void;
  onManualSpeech?: (text: string) => void;
  onOpenModelSettings?: () => void;
  initialSuggestedText?: string;
  onWorkspaceModeChange?: (mode: WorkspaceMode) => void;
  headerActions?: ReactNode;
  showCanvasLink?: boolean;
}

export interface ExecutionController {
  state: AgentRunState;
  messages: Message[];
  persistedMessages?: Message[];
  chainRefreshKey: string;
  pendingApprovals: PendingApproval[];
  resolving: Record<string, boolean>;
  onApprove: (approval: PendingApproval) => void;
  onReject: (approval: PendingApproval) => void;
}

export default function ChatView({
  conversationId,
  onConversationChange,
  showConversationToggle,
  showExecutionToggle,
  conversationToggleRef,
  executionToggleRef,
  onToggleConversations,
  onToggleExecution,
  onVoiceAnchorChange,
  conversationRefreshRevision = 0,
  onOpenCurrentTask,
  renderExecution,
  chatVoiceControls = null,
  onRequestGlobalVoiceSession,
  onManualSpeech,
  onOpenModelSettings,
  initialSuggestedText,
  onWorkspaceModeChange,
  headerActions,
  showCanvasLink,
}: ChatViewProps) {
  const [currentConvId, setCurrentConvId] = useState<string | null>(conversationId);
  const [laneRevision, setLaneRevision] = useState(0);
  const [outputsConversation, setOutputsConversation] = useState<string | null>(null);
  const runtimeId = currentConvId || "__draft__";
  const subscribe = useCallback((listener: () => void) => chatRuntime.subscribe(runtimeId, listener), [runtimeId]);
  const getSnapshot = useCallback(() => chatRuntime.snapshot(runtimeId), [runtimeId]);
  const runtime = useSyncExternalStore(subscribe, getSnapshot);
  const { messages, modelRoute, loading: isLoading, run: runState, pendingApprovals, pendingInputs, resolving } = runtime;
  const { search: locationSearch } = useLocation();
  const sourceMessageId = new URLSearchParams(locationSearch).get("message");
  const locatedSource = useRef<string | null>(null);
  const [localError, setLocalError] = useState<string | null>(null);
  const error = localError || runtime.error;
  const [finished, setFinished] = useState(false);
  const [activeWorkflow, setActiveWorkflow] = useState<Workflow | null>(null);
  const [allWorkflows, setAllWorkflows] = useState<Workflow[]>([]);
  const [suggestedText, setSuggestedText] = useState("");
  const [restoredDraft, setRestoredDraft] = useState<RestoredComposerDraft | null>(null);
  const [retrying, setRetrying] = useState(false);
  const [branching, setBranching] = useState(false);
  const retryInFlight = useRef(false);
  const persistedMessageIds = useMemo(() => new Set(runtime.conversation?.messages.map((message) => message.id) ?? []), [runtime.conversation]);
  const [modelNotice, setModelNotice] = useState<"unavailable" | "unknown" | null>(null);
  const routeNotices = useRef(new Set<string>());
  const messagesScrollRef = useRef<HTMLDivElement>(null);
  const shouldFollowMessagesRef = useRef(true);
  const ownership = useRef(createConversationOwnership(conversationId)).current;
  const routeId = useRef(conversationId);
  const selectLane = useCallback((id: string) => {
    ownership.activate(id);
    setCurrentConvId(id);
    setOutputsConversation(null);
    setLocalError(null);
    setRestoredDraft(null);
    routeNotices.current.clear();
  }, [ownership]);
  useEffect(() => { onWorkspaceModeChange?.(runtime.workspaceMode); }, [runtime.workspaceMode, onWorkspaceModeChange]);
  useEffect(() => {
    if (!runtime.modeNotice?.automatic) return;
    showToast(`已切换到${workspaceModes.find(mode => mode.value === runtime.modeNotice?.mode)?.label}模式${runtime.modeNotice.reason ? `：${runtime.modeNotice.reason}` : ""}`, "info");
  }, [runtime.modeNotice]);
  useEffect(() => {
    if (initialSuggestedText?.trim()) setSuggestedText(initialSuggestedText);
  }, [initialSuggestedText]);
  useLayoutEffect(() => {
    if (routeId.current !== conversationId) {
      routeId.current = conversationId;
      ownership.activate(conversationId);
      setCurrentConvId(conversationId);
      setOutputsConversation(null);
      setLocalError(null);
      setRestoredDraft(null);
      routeNotices.current.clear();
      onVoiceAnchorChange?.(null);
    }
  }, [conversationId, ownership, onVoiceAnchorChange]);

  useEffect(() => () => ownership.invalidate(), [ownership]);

  useEffect(() => {
    if (!runtime.finished) { setFinished(false); return; }
    setFinished(true);
    const timer = window.setTimeout(() => setFinished(false), 1800);
    return () => window.clearTimeout(timer);
  }, [runtime.finished, runtimeId]);

  useEffect(() => {
    if (!modelRoute) return;
    const key = `${modelRoute.attempt_id ?? modelRoute.request_id}:${modelRoute.phase}`;
    if (modelRoute.phase === "selected" || routeNotices.current.has(key)) return;
    routeNotices.current.add(key);
    if (modelRoute.phase === "switched" && modelRoute.from_tier === modelRoute.to_tier) return;
    showToast(modelRoute.phase === "switched" ? `模型已切换到第 ${modelRoute.to_tier} 级，原因：${modelRouteReasonLabel(modelRoute.reason ?? "")}`
      : modelRoute.phase === "recovered" ? "已恢复优先模型" : "本轮模型池所有候选均不可用", modelRoute.phase === "exhausted" ? "error" : "info");
  }, [modelRoute]);

  const loadWorkflows = useCallback(() => {
    listWorkflows().then((result) => {
      if (!result) return;
      setAllWorkflows(result.workflows);
      setActiveWorkflow(
        result.workflows.find((workflow) => workflow.id === result.active_id) ||
          null
      );
    });
  }, []);

  useEffect(() => {
    loadWorkflows();
  }, [loadWorkflows]);

  useEffect(() => {
    let active = true;
    void getChatModelReadiness(currentConvId).then((readiness) => {
      if (active) setModelNotice(readiness ? readiness.available ? null : "unavailable" : "unknown");
    });
    return () => { active = false; };
  }, [currentConvId]);

  useEffect(() => {
    shouldFollowMessagesRef.current = true;
    if (currentConvId) void chatRuntime.refresh(currentConvId);
  }, [currentConvId, conversationRefreshRevision]);

  useEffect(() => {
    const conversation = runtime.conversation;
    if (conversation) onVoiceAnchorChange?.({ conversation_id: conversation.id, title: conversation.title, updated_at: conversation.updated_at });
  }, [runtime.conversation, onVoiceAnchorChange]);

  const [isNearBottomState, setIsNearBottomState] = useState(true);

  useEffect(() => {
    if (!shouldFollowMessagesRef.current) return;
    scrollMessageListToBottom(messagesScrollRef.current);
  }, [messages, runState.content, runState.reasoning, runState.order.length]);

  const handleMessageScroll = useCallback(
    (event: React.UIEvent<HTMLDivElement>) => {
      const near = isNearBottom(event.currentTarget, 48);
      shouldFollowMessagesRef.current = near;
      setIsNearBottomState(near);
    },
    []
  );

  const scrollToLatest = useCallback(() => {
    shouldFollowMessagesRef.current = true;
    setIsNearBottomState(true);
    scrollMessageListToBottom(messagesScrollRef.current, "smooth");
  }, []);

  const streaming = useMemo<StreamingState | null>(() => {
    const visible = isLoading || (runtime.live && !runState.terminal);
    if (!visible) return null;
    return {
      content: runtime.live ? runState.content : "",
      reasoning: runtime.live ? runState.reasoning : "",
      toolCalls: runtime.live ? toToolCallRecords(runState) : [],
    };
  }, [isLoading, runState, runtime.live]);

  const handleApprove = useCallback((approval: PendingApproval) => {
    void chatRuntime.decide(approval, "approve").catch(() => undefined);
  }, []);
  const handleReject = useCallback((approval: PendingApproval) => {
    void chatRuntime.decide(approval, "reject").catch(() => undefined);
  }, []);

  const handleSend = useCallback(
    async (text: string, resourceIds: string[] = [], options?: ChatRequestOptions, workflowId?: string | null) => {
      if ((!text.trim() && !resourceIds.length) || runtime.historyMutation || branching) return false;
      if (/^(?:新任务|并行任务|闲聊|并行聊天)\s*[:：]\s*$/.test(text.trim())) return false;
      let ticket = ownership.capture();
      if (currentConvId && !resourceIds.length && text.trim()) {
        try {
          const reply = await sendChatTaskCommand(currentConvId, text);
          if (!ownership.current(ticket)) return false;
          if (reply) {
            showToast(reply, "info");
            return true;
          }
        } catch (error) {
          if (ownership.current(ticket)) showToast(error instanceof Error ? error.message : "无法读取任务状态", "error");
          return false;
        }
      }
      const question = pendingInputs[0];
      const supplement = text.match(/^\s*(?:补充回答|回答当前问题)\s*[:：]\s*([\s\S]+)$/)?.[1].trim();
      if (currentConvId && pendingInputs.length === 1 && question && supplement && !resourceIds.length && (question.kind === "text" || question.allow_text)) {
        return chatRuntime.answer(currentConvId, question, { revision: question.revision, text: supplement });
      }
      const correction = text.match(/^\s*(?:补充给当前任务|纠正当前任务)\s*[:：]\s*([\s\S]+)$/)?.[1].trim();
      if (currentConvId && runtime.steeringTurnId && isLoading && correction && !resourceIds.length) {
        try {
          await steerChat(currentConvId, runtime.steeringTurnId, correction);
          if (ownership.current(ticket)) showToast("已收到补充，当前步骤结束后纳入处理。", "info");
          return ownership.current(ticket);
        } catch (error) {
          if (ownership.current(ticket)) showToast(error instanceof Error ? error.message : "补充未发送", "error");
          return false;
        }
      }
      const explicitWork = text.match(/^\s*(?:新任务|并行任务)\s*[:：]\s*([\s\S]+)$/)?.[1].trim();
      const explicitChat = text.match(/^\s*(?:闲聊|并行聊天)\s*[:：]\s*([\s\S]+)$/)?.[1].trim();
      const busy = isLoading || pendingApprovals.length > 0 || pendingInputs.length > 0;
      if (correction || supplement) {
        showToast("请先选择对应的运行或待回答问题。", "info");
        return false;
      }
      const inParallel = Boolean(conversationId && (explicitWork || explicitChat || busy));
      if (busy && !inParallel) {
        const hint = question
          ? question.kind === "text" || question.allow_text
            ? "可用「补充回答：…」发送给当前问题，也可在问题卡片里回答。"
            : "请在当前问题卡片中选择答案。"
          : runtime.steeringTurnId
            ? "可用「补充给当前任务：…」更新要求，也可查询进度或取消。"
            : "当前轮次仍在处理中。你可以查询进度或取消，其他内容已留在输入框。";
        showToast(hint, "info");
        return false;
      }
      if (!options?.model_profile_id && !options?.agent_id) {
        const readiness = await getChatModelReadiness(currentConvId);
        if (!ownership.current(ticket)) return false;
        if (!readiness?.available) { setModelNotice(readiness ? "unavailable" : "unknown"); return false; }
      }
      setModelNotice(null);
      const resources = await Promise.all(resourceIds.map(id => getResource(id)));
      if (!ownership.current(ticket)) return false;
      if (resources.some(resource => !resource)) throw new Error("附件元数据读取失败，请重新发送");
      const attachments = resources.map(resource => ({ id: resource!.id, name: resource!.name, mime_type: resource!.mime_type, size: resource!.size }));
      let targetId = currentConvId;
      let parallelOptions = options;
      const message = explicitWork || explicitChat || text;
      if (inParallel && conversationId) {
        const kind = explicitChat ? "chat" : explicitWork || resourceIds.length ? "work" : "auto";
        try {
          const lane = await createConversationLane(conversationId, message, kind);
          if (!ownership.current(ticket)) return false;
          targetId = lane.id;
          parallelOptions = { ...options, workspace_mode: lane.kind === "chat" ? "chat" : explicitWork ? "work" : "auto" };
        } catch (error) {
          if (ownership.current(ticket)) showToast(error instanceof Error ? error.message : "启动并行运行失败", "error");
          return false;
        }
      }
      let promotedDraft = false;
      if (!targetId) {
        const created = await createConversation(text.trim().slice(0, 80) || "附件对话", { useDefaultWorkspace: true });
        if (!ownership.current(ticket)) return false;
        targetId = created.id;
        // The composer recreates its attachment queue when its conversation changes.
        // Preserve the submitted draft until the new conversation accepts it.
        setRestoredDraft({ id: crypto.randomUUID(), text, resources: attachments });
        promotedDraft = true;
        ownership.promote(targetId);
        ticket = ownership.capture();
        routeId.current = targetId;
        setCurrentConvId(targetId);
        onConversationChange(targetId);
      }
      const removedRestoredResources = new Set(inParallel ? [] : restoredDraft?.resources.filter((resource) => !resourceIds.includes(resource.id)).map((resource) => resource.id) ?? []);
      if (removedRestoredResources.size > 0 || resourceIds.length > 0) {
        const bindings = await listResourceBindings({ kind: "conversation", conversation_id: targetId });
        if (!ownership.current(ticket)) return false;
        for (const binding of bindings) {
          if (removedRestoredResources.has(binding.resource_id)) await unbindResource(binding.id);
        }
        const existingIds = new Set(bindings.map((binding) => binding.resource_id));
        const newResourceIds = resourceIds.filter((resourceId) => !existingIds.has(resourceId));
        if (newResourceIds.length > 0) await bindResources({ kind: "conversation", conversation_id: targetId }, newResourceIds);
      }
      if (!ownership.current(ticket)) return false;
      shouldFollowMessagesRef.current = true;
      setLocalError(null);
      const accepted = chatRuntime.send(targetId, inParallel ? message : text, parallelOptions?.workspace_mode === "chat" && inParallel ? null : workflowId === undefined ? activeWorkflow?.id : workflowId, {...parallelOptions,resource_ids:resourceIds}, attachments);
      if (accepted && inParallel) {
        selectLane(targetId);
        setLaneRevision(value => value + 1);
      }
      if (accepted) setRestoredDraft(promotedDraft ? { id: crypto.randomUUID(), text: "", resources: [] } : null);
      return accepted;
    }, [activeWorkflow, conversationId, currentConvId, isLoading, runtime.historyMutation, runtime.steeringTurnId, pendingApprovals.length, pendingInputs, onConversationChange, ownership, restoredDraft, branching, selectLane]
  );

  const handleRollback = useCallback((message: Message) => {
    if (!currentConvId) return;
    const ticket = ownership.capture();
    setLocalError(null);
    void chatRuntime.rollback(currentConvId, message.id).then((result) => {
      if (!ownership.current(ticket)) return;
      notifyConversationHistoryChanged();
      setSuggestedText("");
      setRestoredDraft({ id: crypto.randomUUID(), text: result.draft.text, resources: result.draft.resources });
      shouldFollowMessagesRef.current = true;
    }).catch((cause: unknown) => {
      if (ownership.current(ticket)) setLocalError(cause instanceof Error ? cause.message : String(cause));
    });
  }, [currentConvId, ownership]);

  const handleBranch = useCallback((message: Message) => {
    if (!currentConvId || branching || isLoading || runtime.live || runtime.historyMutation) return;
    const ticket = ownership.capture();
    setBranching(true); setLocalError(null);
    void createConversationBranch(currentConvId, message.id).then(created => {
      if (ownership.current(ticket)) onConversationChange(created.id);
    }).catch((cause: unknown) => {
      if (ownership.current(ticket)) setLocalError(cause instanceof Error ? cause.message : String(cause));
    }).finally(() => setBranching(false));
  }, [currentConvId, branching, isLoading, runtime.live, runtime.historyMutation, ownership, onConversationChange]);

  const handleDeleteMessage = useCallback((message: Message) => {
    if (!currentConvId) return;
    const ticket = ownership.capture();
    setLocalError(null);
    void chatRuntime.deleteMessage(currentConvId, message.id).then(() => {
      notifyConversationHistoryChanged();
    }).catch((cause: unknown) => {
      if (ownership.current(ticket)) setLocalError(cause instanceof Error ? cause.message : String(cause));
    });
  }, [currentConvId, ownership]);

  const locateMessage = useCallback((id: string, behavior: ScrollBehavior) => {
    const viewport = messagesScrollRef.current;
    if (!viewport) return;
    const target = Array.from(viewport.querySelectorAll<HTMLElement>("[data-message-id]"))
      .find((element) => element.dataset.messageId === id);
    if (!target) return false;
    const viewportRect = viewport.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    const offset = targetRect.top - viewportRect.top - viewport.clientTop;
    const centering = (viewport.clientHeight - Math.min(targetRect.height, viewport.clientHeight)) / 2;
    const top = Math.max(0, Math.min(viewport.scrollHeight - viewport.clientHeight, viewport.scrollTop + offset - centering));
    shouldFollowMessagesRef.current = false;
    viewport.scrollTo({ top, behavior });
    target.focus({ preventScroll: true });
    return true;
  }, []);

  const handleLocateMessage = useCallback(() => {
    if (runtime.affectedUserMessageId) locateMessage(runtime.affectedUserMessageId, "smooth");
  }, [locateMessage, runtime.affectedUserMessageId]);

  useEffect(() => {
    if (!sourceMessageId) { locatedSource.current = null; return; }
    const source = `${currentConvId}:${sourceMessageId}`;
    if (currentConvId === conversationId && locatedSource.current !== source &&
      locateMessage(sourceMessageId, "instant")) locatedSource.current = source;
  }, [sourceMessageId, currentConvId, conversationId, messages, locateMessage]);

  const handleRetry = useCallback(() => {
    const submission = runtime.lastSubmission;
    if (!submission || (!submission.text.trim() && !submission.attachments.length) || isLoading || runtime.live || retryInFlight.current || runtime.historyMutation || pendingApprovals.length || pendingInputs.length) return;
    const ticket = ownership.capture();
    retryInFlight.current = true;
    setRetrying(true);
    setLocalError(null);
    void (async () => {
      let text = submission.text;
      let resourceIds = submission.attachments.map((resource) => resource.id);
      if (currentConvId && runtime.retryMessageId) {
        const result = await chatRuntime.rollback(currentConvId, runtime.retryMessageId);
        if (!ownership.current(ticket)) return;
        notifyConversationHistoryChanged();
        text = result.draft.text;
        resourceIds = result.draft.resources.map((resource) => resource.id);
        setSuggestedText("");
        setRestoredDraft({ id: crypto.randomUUID(), text, resources: result.draft.resources });
        // Historical turns do not persist model, thinking, or workflow options.
        // Restore the input for the user to choose them instead of inventing them.
        if (!submission.options) return;
      }
      if (!ownership.current(ticket)) return;
      const accepted = await handleSend(text, resourceIds, submission.options, submission.workflowId ?? null);
      if (accepted && ownership.current(ticket)) setRestoredDraft({ id: crypto.randomUUID(), text: "", resources: [] });
    })().catch((cause: unknown) => {
      if (ownership.current(ticket)) setLocalError(cause instanceof Error ? cause.message : String(cause));
    }).finally(() => { retryInFlight.current = false; setRetrying(false); });
  }, [handleSend, isLoading, runtime.live, runtime.lastSubmission, runtime.retryMessageId, runtime.historyMutation, currentConvId, ownership, pendingApprovals.length, pendingInputs.length]);

  const handleStop = useCallback(async () => {
    if (currentConvId) await chatRuntime.stop(currentConvId);
  }, [currentConvId]);
  const handleWorkspaceMode = useCallback(async (mode: WorkspaceMode) => {
    await chatRuntime.changeWorkspaceMode(runtimeId, mode);
  }, [runtimeId]);

  const handleSelectWorkflow = useCallback((workflow: Workflow | null) => {
    if (!workflow) {
      setActiveWorkflow(null);
      return;
    }
    void activateWorkflow(workflow.id).then((result) => {
      if (result) setActiveWorkflow(workflow);
    });
  }, []);

  const chatModeClass =
    conversationId || messages.length > 0 || streaming || error
      ? "conversation-chat"
      : "";
  const showHome = messages.length === 0 && !streaming && !error;

  return (
    <>
      <div className={`chat-region ${CHAT_COLUMN_VIEWPORT_CLASS_NAME} ${chatModeClass}`} data-workspace-mode={runtime.workspaceMode}>
        <ChatHeader
          panelControls={<>
            {conversationId && currentConvId && <ConversationLanes key={`lanes:${conversationId}`} root={conversationId} selected={currentConvId}
              onSelect={selectLane} onStartWork={() => setSuggestedText("新任务：")} revision={laneRevision} />}
            {currentConvId ? <ConversationWorkspaceControl key={`workspace:${currentConvId}`} conversationId={currentConvId} /> : <DefaultWorkspaceLink />}
            {currentConvId && <button type="button" className="chat-header-outputs inline-flex items-center gap-1 rounded-lg px-2 py-1.5 text-xs text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
              onClick={() => setOutputsConversation(currentConvId)} aria-label="成果" title="查看当前对话成果"><FolderOpen size={14} aria-hidden="true"/><span className="chat-header-action-label">成果</span></button>}
            {headerActions}
          </>}
          title={runtime.conversation?.title || "智能工作台"}
          connection={runState.connection}
          activeWorkflow={activeWorkflow}
          workflows={allWorkflows}
          executionCount={runState.order.length}
          showConversationToggle={showConversationToggle}
          showExecutionToggle={showExecutionToggle}
          conversationToggleRef={conversationToggleRef}
          executionToggleRef={executionToggleRef}
          onToggleConversations={onToggleConversations}
          onToggleExecution={onToggleExecution}
          onSelectWorkflow={handleSelectWorkflow}
          finished={finished}
          home={showHome}
          showCanvasLink={showCanvasLink}
        />

        {currentConvId && <ConversationGoalControl key={currentConvId} conversationId={currentConvId} busy={isLoading || runtime.live || pendingApprovals.length > 0 || pendingInputs.length > 0} />}
        {isLoading && runtime.interaction?.scope && (
          <p role="status" className="shrink-0 px-4 py-1 text-xs text-[var(--text-muted)]">
            {runtime.interaction.exchange === "mixed" ? "正在回应并处理请求" : runtime.interaction.scope === "software_read" ? "正在观察软件窗口" : ["software_control", "wechat_draft"].includes(runtime.interaction.scope) ? "正在操作软件窗口" : runtime.interaction.scope === "work" ? "正在处理工作" : runtime.interaction.scope === "read" ? "正在查阅上下文" : "正在回复"}
          </p>
        )}
        {modelNotice && (
          <section className="chat-model-notice" role="status" aria-label={modelNotice === "unknown" ? "模型状态提醒" : "模型配置提醒"}>
            <p className="font-medium text-[var(--warning-fg)]">{modelNotice === "unknown" ? "暂时无法确认聊天模型状态，请检查本地服务后重试。" : "尚未配置可用的聊天模型。"}</p>
            <button type="button" onClick={onOpenModelSettings}>
              前往模型设置
            </button>
          </section>
        )}
        <ModelRouteNotice route={modelRoute}/>
        {pendingInputs.map((question) => <UserInputCard key={question.question_id} question={question} pending={Boolean(resolving[question.question_id])} onAnswer={(answer) => { if (currentConvId) chatRuntime.answer(currentConvId, question, answer); }} onCancel={() => { if (currentConvId) void chatRuntime.stop(currentConvId); }} />)}

        {pendingApprovals.length > 0 && (
          <div
            className="mx-4 mt-2 flex items-center justify-between gap-3 rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-soft)] px-4 py-2.5 text-xs text-[var(--warning-fg)] shadow-sm animate-fade-in"
            role="status"
          >
            <div className="flex items-center gap-2">
              <ShieldAlert className="h-4 w-4 text-[var(--warning)] shrink-0" />
              <span>有 {pendingApprovals.length} 项操作等待确认</span>
            </div>
            <button
              type="button"
              onClick={onToggleExecution}
              className="font-medium text-[var(--accent-primary)] hover:underline focus-visible:outline-none"
            >
              查看待确认
            </button>
          </div>
        )}

      {showHome ? (
        <WorkbenchHome
          workspaceMode={runtime.workspaceMode}
          onWorkspaceModeChange={handleWorkspaceMode}
          modeChanging={runtime.modeChanging}
          modeError={runtime.modeError}
          connection={runState.connection}
          pendingApprovals={pendingApprovals}
          isLoading={isLoading}
          finished={finished}
          onSend={handleSend}
          conversationId={currentConvId}
          onStop={handleStop}
          suggestedText={suggestedText}
          onTextUsed={() => setSuggestedText("")}
          restoredDraft={restoredDraft}
          inputDisabled={Boolean(runtime.historyMutation) || retrying}
          onHint={setSuggestedText}
          onOpenCurrentTask={onOpenCurrentTask}
          chatVoiceControls={chatVoiceControls}
          onRequestGlobalVoiceSession={onRequestGlobalVoiceSession}
        />
      ) : (
        <>
          <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
            <MessageList
              conversationId={currentConvId}
              messages={messages}
              streaming={streaming}
              scrollContainerRef={messagesScrollRef}
              onScroll={handleMessageScroll}
              error={error}
              onRetry={!isLoading && !runtime.live && !runtime.historyMutation && !runtime.affectedUserMessageId && !retrying && !pendingApprovals.length && !pendingInputs.length && runtime.lastSubmission && (runtime.lastSubmission.text.trim() || runtime.lastSubmission.attachments.length) ? handleRetry : undefined}
              retryLabel={runtime.retryMessageId ? runtime.lastSubmission?.options ? "回退并重试" : "回退并恢复输入" : "重新尝试"}
              onLocateMessage={runtime.affectedUserMessageId && persistedMessageIds.has(runtime.affectedUserMessageId) ? handleLocateMessage : undefined}
              onSpeak={onManualSpeech}
              onRollback={handleRollback}
              onDelete={handleDeleteMessage}
              onBranch={handleBranch}
              persistedMessageIds={persistedMessageIds}
              actionsDisabled={isLoading || runtime.live || Boolean(runtime.historyMutation) || retrying || branching || pendingApprovals.length > 0 || pendingInputs.length > 0}
            />

            {!isNearBottomState && (
              <button
                type="button"
                onClick={scrollToLatest}
                className="chat-scroll-bottom-btn absolute bottom-3 right-6 z-20 flex items-center gap-1.5 rounded-full border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3 py-1.5 text-xs font-medium text-[var(--text-primary)] shadow-md transition-all hover:bg-[var(--surface-hover)]"
                aria-label="回到最新"
                title="回到最新"
              >
                <ArrowDown className="h-3.5 w-3.5" />
                <span>回到最新</span>
              </button>
            )}
          </div>

          <footer className="conversation-composer shrink-0 px-3 pb-3 pt-2 min-[960px]:px-5 min-[960px]:pb-5">
            <div className="conversation-composer-column">
            {branching && <p className="mb-1 px-2 text-xs text-[var(--text-muted)]" role="status">正在创建分支…</p>}
            <ChatInput
              key={currentConvId ?? "draft"}
              workspaceMode={runtime.workspaceMode}
              onWorkspaceModeChange={handleWorkspaceMode}
              modeChanging={runtime.modeChanging}
              modeError={runtime.modeError}
              onSend={handleSend}
              conversationId={currentConvId}
              isLoading={isLoading}
              allowSendWhileRunning={Boolean(currentConvId)}
              acceptsSteering={Boolean(runtime.steeringTurnId)}
              onStop={handleStop}
              suggestedText={suggestedText}
              onTextUsed={() => setSuggestedText("")}
              restoredDraft={restoredDraft}
              disabled={Boolean(runtime.historyMutation) || retrying || branching}
              chatVoiceControls={chatVoiceControls}
              onRequestGlobalVoiceSession={onRequestGlobalVoiceSession}
            />
            </div>
          </footer>
        </>
      )}

      </div>
      {outputsConversation && outputsConversation === currentConvId && <ConversationOutputsModal key={outputsConversation} conversationId={outputsConversation} onClose={() => setOutputsConversation(null)} canRequestDownload={!isLoading && pendingApprovals.length === 0 && pendingInputs.length === 0} onRequestDownload={id => handleSend(`请继续下载当前对话已生成的图片，调用 image_generation，参数仅传 output_id：${id}。完成真实资源登记，不要重新生成图片。`)}/>}
      {renderExecution?.({
        state: runState,
        messages,
        persistedMessages: runtime.conversation?.messages,
        chainRefreshKey: `${runtime.live}:${runState.terminal}:${runState.order.length}:${pendingApprovals.length}:${runtime.conversation?.run_status ?? ""}:${runtime.conversation?.updated_at ?? 0}`,
        pendingApprovals,
        resolving,
        onApprove: handleApprove,
        onReject: handleReject,
      })}
    </>
  );
}
