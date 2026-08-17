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
import { approveAction, rejectAction } from "../../api/approvals";
import {
  createInitialAgentWorkspaceState,
  reduceExecutionWorkspace,
  selectActiveRun,
  toToolCallRecords,
} from "../../features/execution/reducer";
import { createDecisionGate } from "../../features/execution/decisionGate";
import {
  selectPendingApprovals,
  useApprovalStore,
} from "../../stores/approvalStore";
import type { Message } from "../../types";
import type { PendingApproval, RiskLevel } from "../../types/approval";
import type { AgentRunState } from "../../features/execution/model";
import {
  CHAT_COLUMN_VIEWPORT_CLASS_NAME,
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
    : "high";
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
  const messagesScrollRef = useRef<HTMLDivElement>(null);
  const unknownEventTypes = useRef(new Set<string>());

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
    let cancelled = false;
    dispatchExecution({ type: "activate_conversation", conversationId });

    if (!conversationId) {
      setMessages([]);
      setCurrentConvId(null);
      return;
    }

    setCurrentConvId(conversationId);
    loadConversation(conversationId).then((conversation) => {
      if (cancelled || !conversation?.messages) return;
      const loadedMessages = conversation.messages as Message[];
      setMessages(loadedMessages);
      dispatchExecution({
        type: "hydrate_history",
        conversationId,
        toolCalls: loadedMessages.flatMap((message) => message.tool_calls || []),
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
              reason: event.reason || "该操作需要明确授权",
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
          break;
        case "error":
          setIsLoading(false);
          abortRef.current = null;
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
    scrollMessageListToBottom(messagesScrollRef.current);
  }, [messages, runState.content, runState.order.length]);

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
      setError(null);
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

  return (
    <>
      <div className={CHAT_COLUMN_VIEWPORT_CLASS_NAME}>
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
        />

      {messages.length === 0 && !streaming && !error ? (
        <WorkbenchHome
          connection={runState.connection}
          pendingApprovals={pendingApprovals}
          isLoading={isLoading}
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
            onHint={setSuggestedText}
            error={error}
          />

          <footer className="shrink-0 px-3 pb-3 pt-2 min-[960px]:px-5 min-[960px]:pb-5">
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
