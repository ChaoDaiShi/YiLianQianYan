import {
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import {
  activateWorkflow,
  listWorkflows,
  loadConversation,
  sendMessage,
  stopGeneration,
  type AgentEvent,
  type Workflow,
} from "../../api/client";
import {
  approveAction,
  getApproval,
  listPendingApprovals,
  rejectAction,
} from "../../api/approvals";
import {
  createInitialAgentWorkspaceState,
  hydratePersistedExecutionRecords,
  reduceExecutionWorkspace,
  selectActiveRun,
  toToolCallRecords,
} from "../../features/execution/reducer";
import { createDecisionGate } from "../../features/execution/decisionGate";
import { reconcilePersistedApproval } from "../../features/execution/approvalReconciliation";
import {
  selectPendingApprovals,
  useApprovalStore,
} from "../../stores/approvalStore";
import type { Message } from "../../types";
import type { PendingApproval, RiskLevel } from "../../types/approval";
import type { AgentRunState } from "../../features/execution/model";
import {
  CHAT_COLUMN_VIEWPORT_CLASS_NAME,
  isNearBottom,
  scrollMessageListToBottom,
} from "../layout/workspaceLayout";
import ChatInput from "./ChatInput";
import ChatHeader from "./ChatHeader";
import MessageList, { type StreamingState } from "./MessageList";
import WorkbenchHome from "./WorkbenchHome";

interface ChatViewProps {
  conversationId: string | null;
  onConversationChange: (id: string | null) => void;
  showConversationToggle: boolean;
  showExecutionToggle: boolean;
  conversationToggleRef?: RefObject<HTMLButtonElement>;
  executionToggleRef?: RefObject<HTMLButtonElement>;
  onToggleConversations: () => void;
  onToggleExecution: () => void;
  renderExecution?: (controller: ExecutionController) => ReactNode;
}

export interface ExecutionController {
  state: AgentRunState;
  pendingApprovals: PendingApproval[];
  resolving: Record<string, boolean>;
  onApprove: (approval: PendingApproval) => void;
  onReject: (approval: PendingApproval) => void;
}

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

function riskLevel(value: string | undefined): RiskLevel {
  return value === "low" ||
    value === "medium" ||
    value === "high" ||
    value === "critical"
    ? value
    : "unknown";
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
  renderExecution,
}: ChatViewProps) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [currentConvId, setCurrentConvId] = useState<string | null>(
    conversationId
  );
  const [error, setError] = useState<string | null>(null);
  const [finished, setFinished] = useState(false);
  const [lastSubmittedText, setLastSubmittedText] = useState("");
  const [activeWorkflow, setActiveWorkflow] = useState<Workflow | null>(null);
  const [allWorkflows, setAllWorkflows] = useState<Workflow[]>([]);
  const [suggestedText, setSuggestedText] = useState("");
  const [execution, dispatchExecution] = useReducer(
    reduceExecutionWorkspace,
    conversationId,
    createInitialAgentWorkspaceState
  );
  const runState = selectActiveRun(execution);
  const pendingApprovals = useApprovalStore(selectPendingApprovals);
  const resolving = useApprovalStore((state) => state.resolving);
  const abortRef = useRef<AbortController | null>(null);
  const finishedTimerRef = useRef<number | null>(null);
  const messagesScrollRef = useRef<HTMLDivElement>(null);
  const shouldFollowMessagesRef = useRef(true);
  const unknownEventTypes = useRef(new Set<string>());

  useEffect(() => {
    return () => {
      if (finishedTimerRef.current !== null) {
        window.clearTimeout(finishedTimerRef.current);
      }
    };
  }, []);

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
    shouldFollowMessagesRef.current = true;
    let cancelled = false;
    setFinished(false);
    if (finishedTimerRef.current !== null) {
      window.clearTimeout(finishedTimerRef.current);
      finishedTimerRef.current = null;
    }
    dispatchExecution({ type: "activate_conversation", conversationId });

    if (!conversationId) {
      setMessages([]);
      setCurrentConvId(null);
      return;
    }

    setCurrentConvId(conversationId);
    loadConversation(conversationId).then(async (conversation) => {
      if (cancelled || !conversation?.messages) return;
      const loadedMessages = (conversation.messages as unknown[]).filter(
        (message): message is Message => {
          if (!message || typeof message !== "object") return false;
          const candidate = message as Partial<Message>;
          return (
            typeof candidate.id === "string" &&
            typeof candidate.role === "string" &&
            typeof candidate.content === "string" &&
            candidate.content.trim().length > 0
          );
        }
      );
      setMessages(loadedMessages);
      dispatchExecution({
        type: "hydrate_history",
        conversationId,
        toolCalls: loadedMessages.flatMap((message) => message.tool_calls || []),
        executionHistory: conversation.execution_history,
      });

      const persistedRecords = hydratePersistedExecutionRecords(
        conversation.execution_history,
        conversationId
      );
      const approvalRecords = persistedRecords.filter(
        (record) => record.approvalStatus === "pending" && record.approvalId
      );
      const [listedApprovals, ...lookups] = await Promise.all([
        listPendingApprovals(),
        ...approvalRecords.map((record) =>
          getApproval(record.approvalId!).then(
            (approval) => ({ approval, unavailable: false }),
            () => ({ approval: null, unavailable: true })
          )
        ),
      ]);
      if (cancelled) return;

      const store = useApprovalStore.getState();
      (listedApprovals || [])
        .filter((approval) => approval.conversation_id === conversationId)
        .forEach((approval) => store.add(approval));

      approvalRecords.forEach((record, index) => {
        const lookup = lookups[index] as
          | { approval: PendingApproval | null; unavailable: boolean }
          | undefined;
        if (!lookup || lookup.unavailable) return;

        const reconciliation = reconcilePersistedApproval(
          record,
          lookup.approval
        );
        if (reconciliation.kind === "pending") {
          store.add(reconciliation.approval);
        } else if (reconciliation.kind === "resolved") {
          store.remove(record.approvalId!);
          dispatchExecution({ type: "agent_event", event: reconciliation.event });
        }
      });
    });

    return () => {
      cancelled = true;
    };
  }, [conversationId]);

  const clearResolvingForConversation = useCallback((targetId: string) => {
    const store = useApprovalStore.getState();
    selectPendingApprovals(store)
      .filter((approval) => approval.conversation_id === targetId)
      .forEach((approval) =>
        store.markResolving(approval.approval_id, false)
      );
  }, []);

  const handleAgentEvent = useCallback(
    (event: AgentEvent) => {
      if (!KNOWN_EVENT_TYPES.has(event.type)) {
        if (!unknownEventTypes.current.has(event.type)) {
          unknownEventTypes.current.add(event.type);
          console.debug("Ignoring unknown agent event:", event.type);
        }
        return;
      }

      dispatchExecution({ type: "agent_event", event });

      switch (event.type) {
        case "connected":
          if (event.conversation_id) {
            setCurrentConvId(event.conversation_id);
            if (!conversationId) onConversationChange(event.conversation_id);
          }
          break;
        case "approval_required":
          if (event.approval_id) {
            useApprovalStore.getState().add({
              approval_id: event.approval_id,
              conversation_id: event.conversation_id,
              tool_call_id: event.tool_call_id || "",
              tool_name: event.tool_name || "",
              arguments: event.args || {},
              risk_level: riskLevel(event.risk_level),
              reason: event.reason || "",
              status: "pending",
              created_at: new Date().toISOString(),
              expires_at: "",
            });
          }
          break;
        case "approval_resolved":
          if (event.approval_id) {
            useApprovalStore.getState().remove(event.approval_id);
          }
          break;
        case "stream_end":
          setIsLoading(false);
          abortRef.current = null;
          clearResolvingForConversation(event.conversation_id);
          break;
        case "done":
          setIsLoading(false);
          abortRef.current = null;
          setFinished(true);
          if (finishedTimerRef.current !== null) {
            window.clearTimeout(finishedTimerRef.current);
          }
          finishedTimerRef.current = window.setTimeout(() => {
            finishedTimerRef.current = null;
            setFinished(false);
          }, 1800);
          break;
        case "error":
          setIsLoading(false);
          abortRef.current = null;
          setFinished(false);
          setError(event.error || "生成失败，请重试");
          clearResolvingForConversation(event.conversation_id);
          break;
      }
    },
    [clearResolvingForConversation, conversationId, onConversationChange]
  );

  const decisionContextRef = useRef({ currentConvId, handleAgentEvent });
  useEffect(() => {
    decisionContextRef.current = { currentConvId, handleAgentEvent };
  }, [currentConvId, handleAgentEvent]);

  const decisionGateRef = useRef<ReturnType<typeof createDecisionGate> | null>(
    null
  );
  if (!decisionGateRef.current) {
    decisionGateRef.current = createDecisionGate(async (approvalId, decision) => {
      const store = useApprovalStore.getState();
      store.markResolving(approvalId, true);
      setError(null);
      setIsLoading(true);
      try {
        const context = decisionContextRef.current;
        const submit = decision === "approve" ? approveAction : rejectAction;
        await submit(
          approvalId,
          context.currentConvId,
          context.handleAgentEvent
        );
      } catch (requestError) {
        store.markResolving(approvalId, false);
        setIsLoading(false);
        setError(
          requestError instanceof Error
            ? requestError.message
            : "审批请求失败，请重试"
        );
        throw requestError;
      }
    });
  }

  useEffect(() => {
    const completed = execution.completed;
    if (!completed) return;

    if (completed.conversationId === currentConvId) {
      const assistantMessage: Message = {
        id: completed.messageId,
        role: "assistant",
        content: completed.content,
        created_at: Date.now(),
        tool_calls:
          completed.records.length > 0
            ? toToolCallRecords(completed.records)
            : undefined,
      };
      setMessages((current) =>
        current.some((message) => message.id === assistantMessage.id)
          ? current
          : [...current, assistantMessage]
      );
    }
    dispatchExecution({ type: "consume_completed" });
  }, [currentConvId, execution.completed]);

  useEffect(() => {
    if (!shouldFollowMessagesRef.current) return;
    scrollMessageListToBottom(messagesScrollRef.current);
  }, [messages, runState.content, runState.order.length]);

  const handleMessageScroll = useCallback(
    (event: React.UIEvent<HTMLDivElement>) => {
      shouldFollowMessagesRef.current = isNearBottom(event.currentTarget);
    },
    []
  );

  const streaming = useMemo<StreamingState | null>(() => {
    const hasLiveOutput = runState.content.length > 0 || runState.order.length > 0;
    const visible =
      isLoading ||
      (hasLiveOutput && !runState.terminal && runState.connection !== "idle");
    if (!visible) return null;
    return {
      content: runState.content,
      toolCalls: toToolCallRecords(runState),
    };
  }, [isLoading, runState]);

  const handleApprove = useCallback((approval: PendingApproval) => {
    void decisionGateRef.current
      ?.submit(approval.approval_id, "approve")
      .catch(() => undefined);
  }, []);

  const handleReject = useCallback((approval: PendingApproval) => {
    void decisionGateRef.current
      ?.submit(approval.approval_id, "reject")
      .catch(() => undefined);
  }, []);

  const handleSend = useCallback(
    (text: string) => {
      if (!text.trim() || isLoading) return;
      shouldFollowMessagesRef.current = true;
      setError(null);
      setFinished(false);
      setLastSubmittedText(text);
      setMessages((current) => [
        ...current,
        {
          id: crypto.randomUUID(),
          role: "user",
          content: text,
          created_at: Date.now(),
        },
      ]);
      dispatchExecution({ type: "start_run", conversationId: currentConvId });
      setIsLoading(true);
      abortRef.current = sendMessage(
        text,
        currentConvId,
        handleAgentEvent,
        activeWorkflow?.id
      );
    },
    [activeWorkflow, currentConvId, handleAgentEvent, isLoading]
  );

  const handleRetry = useCallback(() => {
    const retryText = lastSubmittedText.trim();
    if (!retryText || isLoading) return;
    setMessages((current) => {
      const last = current[current.length - 1];
      return last?.role === "user" && last.content === retryText
        ? current.slice(0, -1)
        : current;
    });
    handleSend(retryText);
  }, [handleSend, isLoading, lastSubmittedText]);

  const handleStop = useCallback(async () => {
    abortRef.current?.abort();
    if (currentConvId) await stopGeneration(currentConvId);
    handleAgentEvent({
      type: "stream_end",
      conversation_id: currentConvId || "",
    });
  }, [currentConvId, handleAgentEvent]);

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
      <div className={`${CHAT_COLUMN_VIEWPORT_CLASS_NAME} ${chatModeClass}`}>
        <ChatHeader
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

      {messages.length === 0 && !streaming && !error ? (
        <WorkbenchHome
          connection={runState.connection}
          pendingApprovals={pendingApprovals}
          isLoading={isLoading}
          finished={finished}
          onSend={handleSend}
          onStop={handleStop}
          suggestedText={suggestedText}
          onTextUsed={() => setSuggestedText("")}
          onHint={setSuggestedText}
        />
      ) : (
        <>
          <MessageList
            messages={messages}
            streaming={streaming}
            scrollContainerRef={messagesScrollRef}
            onScroll={handleMessageScroll}
            error={error}
            onRetry={handleRetry}
          />

          <footer className="conversation-composer shrink-0 px-3 pb-3 pt-2 min-[960px]:px-5 min-[960px]:pb-5">
            <ChatInput
              onSend={handleSend}
              isLoading={isLoading}
              onStop={handleStop}
              suggestedText={suggestedText}
              onTextUsed={() => setSuggestedText("")}
            />
          </footer>
        </>
      )}

      </div>
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
