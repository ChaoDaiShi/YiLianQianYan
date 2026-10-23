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
import ConversationWorkspaceControl from "./ConversationWorkspaceControl";
import ConversationOutputsModal from "../../features/outputs/ConversationOutputsModal";
import { bindResources, getResource, listResourceBindings, unbindResource } from "../../api/resources";
import { getChatModelReadiness } from "../../api/providerConnection";
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
import WorkbenchHome from "./WorkbenchHome";
import ModelRouteNotice, { modelRouteReasonLabel } from "../../features/llm/ModelRouteNotice";
import { showToast } from "../ui";

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
}

export interface ExecutionController {
  state: AgentRunState;
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
}: ChatViewProps) {
  const [currentConvId, setCurrentConvId] = useState<string | null>(conversationId);
  const [outputsConversation, setOutputsConversation] = useState<string | null>(null);
  const runtimeId = currentConvId || "__draft__";
  const subscribe = useCallback((listener: () => void) => chatRuntime.subscribe(runtimeId, listener), [runtimeId]);
  const getSnapshot = useCallback(() => chatRuntime.snapshot(runtimeId), [runtimeId]);
  const runtime = useSyncExternalStore(subscribe, getSnapshot);
  const { messages, modelRoute, loading: isLoading, run: runState, pendingApprovals, pendingInputs, resolving } = runtime;
  const [localError, setLocalError] = useState<string | null>(null);
  const error = localError || runtime.error;
  const [finished, setFinished] = useState(false);
  const [activeWorkflow, setActiveWorkflow] = useState<Workflow | null>(null);
  const [allWorkflows, setAllWorkflows] = useState<Workflow[]>([]);
  const [suggestedText, setSuggestedText] = useState("");
  const [restoredDraft, setRestoredDraft] = useState<RestoredComposerDraft | null>(null);
  const [retrying, setRetrying] = useState(false);
  const retryInFlight = useRef(false);
  const persistedMessageIds = useMemo(() => new Set(runtime.conversation?.messages.map((message) => message.id) ?? []), [runtime.conversation]);
  const [modelUnavailable, setModelUnavailable] = useState(false);
  const routeNotices = useRef(new Set<string>());
  const messagesScrollRef = useRef<HTMLDivElement>(null);
  const shouldFollowMessagesRef = useRef(true);
  const ownership = useRef(createConversationOwnership(conversationId)).current;
  useEffect(() => { onWorkspaceModeChange?.(runtime.workspaceMode); }, [runtime.workspaceMode, onWorkspaceModeChange]);
  useEffect(() => {
    if (!runtime.modeNotice?.automatic) return;
    showToast(`已切换到${workspaceModes.find(mode => mode.value === runtime.modeNotice?.mode)?.label}模式${runtime.modeNotice.reason ? `：${runtime.modeNotice.reason}` : ""}`, "info");
  }, [runtime.modeNotice]);
  useEffect(() => {
    if (initialSuggestedText?.trim()) setSuggestedText(initialSuggestedText);
  }, [initialSuggestedText]);
  useLayoutEffect(() => {
    if (ownership.id !== conversationId) {
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
      if (active) setModelUnavailable(!readiness?.available);
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
    async (text: string, resourceIds: string[] = [], options?: ChatRequestOptions) => {
      if (!text.trim() || isLoading || runtime.historyMutation || pendingApprovals.length > 0 || pendingInputs.length > 0) return false;
      let ticket = ownership.capture();
      if (!options?.model_profile_id) {
        const readiness = await getChatModelReadiness(currentConvId);
        if (!ownership.current(ticket)) return false;
        if (!readiness?.available) { setModelUnavailable(true); return false; }
      }
      setModelUnavailable(false);
      let targetId = currentConvId;
      if (!targetId) {
        const created = await createConversation(text.trim().slice(0, 80));
        if (!ownership.current(ticket)) return false;
        targetId = created.id;
        ownership.promote(targetId);
        ticket = ownership.capture();
        setCurrentConvId(targetId);
        onConversationChange(targetId);
      }
      const removedRestoredResources = new Set(restoredDraft?.resources.filter((resource) => !resourceIds.includes(resource.id)).map((resource) => resource.id) ?? []);
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
      const resources=await Promise.all(resourceIds.map(id=>getResource(id)));
      if (!ownership.current(ticket)) return false;
      if(resources.some(resource=>!resource))throw new Error("附件元数据读取失败，请重新发送");
      const attachments=resources.map(resource=>({id:resource!.id,name:resource!.name,mime_type:resource!.mime_type,size:resource!.size}));
      const accepted = chatRuntime.send(targetId, text, activeWorkflow?.id, {...options,resource_ids:resourceIds}, attachments);
      if (accepted) setRestoredDraft(null);
      return accepted;
    }, [activeWorkflow, currentConvId, isLoading, runtime.historyMutation, pendingApprovals.length, pendingInputs.length, onConversationChange, ownership, restoredDraft]
  );

  const handleRollback = useCallback((message: Message) => {
    if (!currentConvId) return;
    const ticket = ownership.capture();
    setLocalError(null);
    void chatRuntime.rollback(currentConvId, message.id).then((result) => {
      if (!ownership.current(ticket)) return;
      setSuggestedText("");
      setRestoredDraft({ id: crypto.randomUUID(), text: result.draft.text, resources: result.draft.resources });
      shouldFollowMessagesRef.current = true;
    }).catch((cause: unknown) => {
      if (ownership.current(ticket)) setLocalError(cause instanceof Error ? cause.message : String(cause));
    });
  }, [currentConvId, ownership]);

  const handleDeleteMessage = useCallback((message: Message) => {
    if (!currentConvId) return;
    const ticket = ownership.capture();
    setLocalError(null);
    void chatRuntime.deleteMessage(currentConvId, message.id).catch((cause: unknown) => {
      if (ownership.current(ticket)) setLocalError(cause instanceof Error ? cause.message : String(cause));
    });
  }, [currentConvId, ownership]);

  const handleLocateMessage = useCallback(() => {
    const viewport = messagesScrollRef.current;
    if (!viewport) return;
    const target = Array.from(viewport.querySelectorAll<HTMLElement>("[data-message-id]"))
      .find((element) => element.dataset.messageId === runtime.affectedUserMessageId);
    if (!target) return;
    const viewportRect = viewport.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    const offset = targetRect.top - viewportRect.top - viewport.clientTop;
    const centering = (viewport.clientHeight - Math.min(targetRect.height, viewport.clientHeight)) / 2;
    const top = Math.max(0, Math.min(viewport.scrollHeight - viewport.clientHeight, viewport.scrollTop + offset - centering));
    shouldFollowMessagesRef.current = false;
    viewport.scrollTo({ top, behavior: "smooth" });
    target.focus({ preventScroll: true });
  }, [runtime.affectedUserMessageId]);

  const handleRetry = useCallback(() => {
    if (!runtime.lastSubmittedText.trim() || isLoading || runtime.live || retryInFlight.current || runtime.historyMutation) return;
    const ticket = ownership.capture();
    retryInFlight.current = true;
    setRetrying(true);
    setLocalError(null);
    void (async () => {
      let text = runtime.lastSubmittedText;
      let resourceIds: string[] = [];
      if (currentConvId && runtime.retryMessageId) {
        const result = await chatRuntime.rollback(currentConvId, runtime.retryMessageId);
        if (!ownership.current(ticket)) return;
        text = result.draft.text;
        resourceIds = result.draft.resources.map((resource) => resource.id);
        setSuggestedText("");
        setRestoredDraft({ id: crypto.randomUUID(), text, resources: result.draft.resources });
      }
      if (!ownership.current(ticket)) return;
      const accepted = await handleSend(text, resourceIds);
      if (accepted && ownership.current(ticket)) setRestoredDraft({ id: crypto.randomUUID(), text: "", resources: [] });
    })().catch((cause: unknown) => {
      if (ownership.current(ticket)) setLocalError(cause instanceof Error ? cause.message : String(cause));
    }).finally(() => { retryInFlight.current = false; setRetrying(false); });
  }, [handleSend, isLoading, runtime.live, runtime.lastSubmittedText, runtime.retryMessageId, runtime.historyMutation, currentConvId, ownership]);

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

  return (
    <>
      <div className={`chat-region ${CHAT_COLUMN_VIEWPORT_CLASS_NAME} ${chatModeClass}`} data-workspace-mode={runtime.workspaceMode}>
        <ChatHeader
          panelControls={<>{currentConvId && <ConversationWorkspaceControl key={currentConvId} conversationId={currentConvId} />}{currentConvId && <button type="button" className="inline-flex items-center gap-1 rounded-lg px-2 py-1.5 text-xs text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]" onClick={() => setOutputsConversation(currentConvId)} title="查看当前对话成果"><FolderOpen size={14}/><span>成果</span></button>}{headerActions}</>}
          title="智能工作台"
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
        />

        {modelUnavailable ? (
          <section className="mx-4 mt-3 rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-soft)] px-4 py-3 text-sm" role="status">
            <p className="font-medium text-[var(--warning-fg)]">还没有配置可用的模型服务。</p>
            <button type="button" className="mt-2 text-[var(--accent-primary)] underline" onClick={onOpenModelSettings}>
              前往模型设置
            </button>
          </section>
        ) : null}
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

      {messages.length === 0 && !streaming && !error ? (
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
              messages={messages}
              streaming={streaming}
              scrollContainerRef={messagesScrollRef}
              onScroll={handleMessageScroll}
              error={error}
              onRetry={!runtime.live && !runtime.historyMutation && !runtime.affectedUserMessageId && !retrying ? handleRetry : undefined}
              onLocateMessage={runtime.affectedUserMessageId && persistedMessageIds.has(runtime.affectedUserMessageId) ? handleLocateMessage : undefined}
              onSpeak={onManualSpeech}
              onRollback={handleRollback}
              onDelete={handleDeleteMessage}
              persistedMessageIds={persistedMessageIds}
              actionsDisabled={isLoading || runtime.live || Boolean(runtime.historyMutation) || retrying}
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
            <ChatInput
              workspaceMode={runtime.workspaceMode}
              onWorkspaceModeChange={handleWorkspaceMode}
              modeChanging={runtime.modeChanging}
              modeError={runtime.modeError}
              onSend={handleSend}
              conversationId={currentConvId}
              isLoading={isLoading}
              onStop={handleStop}
              suggestedText={suggestedText}
              onTextUsed={() => setSuggestedText("")}
              restoredDraft={restoredDraft}
              disabled={Boolean(runtime.historyMutation) || retrying}
              chatVoiceControls={chatVoiceControls}
              onRequestGlobalVoiceSession={onRequestGlobalVoiceSession}
            />
          </footer>
        </>
      )}

      </div>
      {outputsConversation && outputsConversation === currentConvId && <ConversationOutputsModal key={outputsConversation} conversationId={outputsConversation} onClose={() => setOutputsConversation(null)} canRequestDownload={!isLoading && pendingApprovals.length === 0 && pendingInputs.length === 0} onRequestDownload={id => handleSend(`请继续下载当前对话已生成的图片，调用 image_generation，参数仅传 output_id：${id}。完成真实资源登记，不要重新生成图片。`)}/>}
      {renderExecution?.({
        state: runState,
        pendingApprovals,
        resolving,
        onApprove: handleApprove,
        onReject: handleReject,
      })}
    </>
  );
}
