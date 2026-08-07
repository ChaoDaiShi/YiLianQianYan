import { useState, useEffect, useRef, useCallback } from "react";
import { Menu, GitBranch, ChevronDown } from "lucide-react";
import {
  sendMessage,
  loadConversation,
  stopGeneration,
  listWorkflows,
  activateWorkflow,
  type AgentEvent,
  type Workflow,
} from "../../api/client";
import { Message } from "../../types";
import { approveAction, rejectAction } from "../../api/approvals";
import { useApprovalStore } from "../../stores/approvalStore";
import type { PendingApproval, RiskLevel } from "../../types/approval";
import MessageList from "./MessageList";
import ChatInput from "./ChatInput";

interface ChatViewProps {
  conversationId: string | null;
  onConversationChange: (id: string | null) => void;
  showSidebar: boolean;
  onToggleSidebar: () => void;
}

export interface StreamingState {
  content: string;
  toolCalls: Map<
    string,
    { name: string; args: Record<string, unknown>; status: "running" | "success" | "error" | "blocked"; result?: string }
  >;
}

export default function ChatView({
  conversationId,
  onConversationChange,
  showSidebar,
  onToggleSidebar,
}: ChatViewProps) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [streaming, setStreaming] = useState<StreamingState | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [currentConvId, setCurrentConvId] = useState<string | null>(conversationId);
  const [error, setError] = useState<string | null>(null);
  const [activeWorkflow, setActiveWorkflow] = useState<Workflow | null>(null);
  const [allWorkflows, setAllWorkflows] = useState<Workflow[]>([]);
  const [showWfMenu, setShowWfMenu] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null!);
  const [suggestedText, setSuggestedText] = useState("");

  const loadWorkflows = useCallback(() => {
    listWorkflows().then((res) => {
      if (!res) return;
      setAllWorkflows(res.workflows);
      const active = res.workflows.find((w) => w.id === res.active_id) || null;
      setActiveWorkflow(active);
    });
  }, []);

  useEffect(() => { loadWorkflows(); }, [loadWorkflows]);

  useEffect(() => {
    if (conversationId) {
      setCurrentConvId(conversationId);
      loadConversation(conversationId).then((conv) => {
        if (conv?.messages) setMessages(conv.messages);
      });
    } else {
      setMessages([]);
      setCurrentConvId(null);
    }
  }, [conversationId]);

  const handleAgentEvent = useCallback(
    (event: AgentEvent) => {
      switch (event.type) {
        case "connected":
          if (event.conversation_id) {
            setCurrentConvId(event.conversation_id);
            if (!conversationId) onConversationChange(event.conversation_id);
          }
          break;
        case "token":
          setStreaming((prev) => {
            if (!prev) return { content: event.token || "", toolCalls: new Map() };
            return { ...prev, content: prev.content + (event.token || "") };
          });
          break;
        case "tool_start":
          setStreaming((prev) => {
            const toolCalls = new Map(prev?.toolCalls || []);
            toolCalls.set(event.tool_call_id || "", {
              name: event.tool_name || "",
              args: event.args || {},
              status: "running",
            });
            return { content: prev?.content || "", toolCalls };
          });
          break;
        case "tool_end":
          setStreaming((prev) => {
            const toolCalls = new Map(prev?.toolCalls || []);
            const existing = toolCalls.get(event.tool_call_id || "");
            if (existing) {
              toolCalls.set(event.tool_call_id || "", {
                ...existing,
                status: event.status === "success" ? "success" : "error",
                result: event.result,
              });
            }
            return { content: prev?.content || "", toolCalls };
          });
          break;
        case "approval_required":
          setStreaming((prev) => {
            const toolCalls = new Map(prev?.toolCalls || []);
            toolCalls.set(event.tool_call_id || "", {
              name: event.tool_name || "",
              args: event.args || {},
              status: "blocked",
              result: `操作需要批准，当前版本尚未执行。原因：${event.reason || "高风险操作"}（风险等级：${event.risk_level || "unknown"}）`,
            });
            return { content: prev?.content || "", toolCalls };
          });
          if (event.approval_id) {
            useApprovalStore.getState().add({
              approval_id: event.approval_id,
              conversation_id: event.conversation_id,
              tool_call_id: event.tool_call_id || "",
              tool_name: event.tool_name || "",
              arguments: event.args || {},
              risk_level: (event.risk_level as RiskLevel) || "high",
              reason: event.reason || "高风险操作",
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
          // Rejected/cancelled tools will never run — reflect that on the card.
          if (event.status && event.status !== "approved") {
            setStreaming((prev) => {
              const toolCalls = new Map(prev?.toolCalls || []);
              const existing = toolCalls.get(event.tool_call_id || "");
              if (existing) {
                toolCalls.set(event.tool_call_id || "", {
                  ...existing,
                  status: "error",
                  result: "用户已拒绝该操作，未执行。",
                });
              }
              return { content: prev?.content || "", toolCalls };
            });
          }
          break;
        case "stream_end":
          // Agent paused awaiting approval, or the stream closed without a
          // terminal event — stop the loading spinner so the user can decide.
          setIsLoading(false);
          break;
        case "done":
          setStreaming((prev) => {
            if (prev) {
              const toolCalls: Array<{
                toolCallId: string;
                name: string;
                args: Record<string, unknown>;
                status: "running" | "success" | "error" | "blocked";
                result?: string;
              }> = [];
              prev.toolCalls.forEach((tc, id) => {
                toolCalls.push({ toolCallId: id, ...tc });
              });
              const newMsg: Message = {
                id: event.message_id || crypto.randomUUID(),
                role: "assistant",
                content: prev.content,
                created_at: Date.now(),
                tool_calls: toolCalls.length > 0 ? toolCalls : undefined,
              };
              setMessages((msgs) => [...msgs, newMsg]);
            }
            return null;
          });
          setIsLoading(false);
          abortRef.current = null;
          break;
        case "error":
          setStreaming(null);
          setIsLoading(false);
          abortRef.current = null;
          setError(event.error || "生成失败");
          break;
      }
    },
    [conversationId, onConversationChange]
  );

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streaming]);

  const handleApprove = useCallback(
    (approval: PendingApproval) => {
      useApprovalStore.getState().markResolving(approval.approval_id, true);
      approveAction(approval.approval_id, currentConvId, handleAgentEvent);
    },
    [currentConvId, handleAgentEvent]
  );

  const handleReject = useCallback(
    (approval: PendingApproval) => {
      useApprovalStore.getState().markResolving(approval.approval_id, true);
      rejectAction(approval.approval_id, currentConvId, handleAgentEvent);
    },
    [currentConvId, handleAgentEvent]
  );

  const handleSend = useCallback(
    async (text: string) => {
      if (!text.trim() || isLoading) return;
      setError(null);
      const userMsg: Message = {
        id: crypto.randomUUID(),
        role: "user",
        content: text,
        created_at: Date.now(),
      };
      setMessages((prev) => [...prev, userMsg]);
      setIsLoading(true);
      const controller = sendMessage(
        text,
        currentConvId,
        handleAgentEvent,
        activeWorkflow?.id
      );
      abortRef.current = controller;
    },
    [currentConvId, isLoading, handleAgentEvent, activeWorkflow]
  );

  const handleStop = async () => {
    abortRef.current?.abort();
    if (currentConvId) await stopGeneration(currentConvId);
    setIsLoading(false);
    setStreaming(null);
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-3 px-4 py-3 border-b border-[var(--border)] bg-[var(--panel)]/60 backdrop-blur-md">
        <button
          onClick={onToggleSidebar}
          className="p-1.5 rounded-lg hover:bg-[var(--panel-hover)] transition-colors text-[var(--text-muted)]"
          title={showSidebar ? "收起侧栏" : "展开侧栏"}
        >
          <Menu className="w-5 h-5" />
        </button>
        <img src="/favicon.png" alt="忆涟千言" className="w-7 h-7 rounded-lg object-cover" />
        <h2 className="font-semibold text-lg font-display">忆涟千言</h2>

        {/* Workflow selector */}
        <div className="relative">
          <button
            onClick={() => setShowWfMenu(!showWfMenu)}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg hover:bg-[var(--panel-hover)] transition-colors text-sm"
            title="切换工作流"
          >
            <GitBranch className="w-3.5 h-3.5 text-[var(--accent)]" />
            <span className="text-[var(--text-muted)] max-w-[120px] truncate">
              {activeWorkflow ? activeWorkflow.name : "无工作流"}
            </span>
            <ChevronDown className="w-3.5 h-3.5 text-[var(--text-faint)]" />
          </button>

          {showWfMenu && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setShowWfMenu(false)} />
              <div className="absolute top-full left-0 mt-1 w-64 rounded-xl border border-[var(--border)] bg-[var(--panel)] shadow-2xl z-50 py-1 max-h-80 overflow-y-auto scrollbar-thin">
                <button
                  onClick={() => {
                    setActiveWorkflow(null);
                    setShowWfMenu(false);
                    // Note: The next message will be sent without workflow_id,
                    // and the backend will use the active workflow from settings.
                    // To explicitly disable, we could call an API, but for now
                    // just not passing workflow_id means no workflow override.
                  }}
                  className={`w-full text-left px-3 py-2 text-sm hover:bg-[var(--panel-hover)] transition-colors ${
                    !activeWorkflow ? "text-[var(--accent)]" : "text-[var(--text-muted)]"
                  }`}
                >
                  不使用工作流
                </button>
                {allWorkflows.map((wf) => (
                  <button
                    key={wf.id}
                    onClick={async () => {
                      await activateWorkflow(wf.id);
                      setActiveWorkflow(wf);
                      setShowWfMenu(false);
                    }}
                    className={`w-full text-left px-3 py-2 text-sm hover:bg-[var(--panel-hover)] transition-colors flex items-center gap-2 ${
                      activeWorkflow?.id === wf.id ? "text-[var(--accent)]" : "text-[var(--text-muted)]"
                    }`}
                  >
                    <span className="text-xs flex-shrink-0">{wf.is_builtin ? "📦" : "⚡"}</span>
                    <span className="truncate">{wf.name}</span>
                    <span className="text-[10px] text-[var(--text-faint)] ml-auto flex-shrink-0">
                      {wf.nodes.length > 0 ? wf.nodes.length + "步" : ""}
                    </span>
                  </button>
                ))}
              </div>
            </>
          )}
        </div>

        <div className="flex items-center gap-2 ml-auto">
          <span className="w-2 h-2 rounded-full bg-[var(--success)]" title="后端已连接" />
          <span className="text-xs text-[var(--text-faint)] font-mono">API</span>
        </div>
      </div>

      <MessageList
        messages={messages}
        streaming={streaming}
        messagesEndRef={messagesEndRef}
        onHint={setSuggestedText}
        error={error}
        onApprove={handleApprove}
        onReject={handleReject}
      />

      <ChatInput
        onSend={handleSend}
        isLoading={isLoading}
        onStop={handleStop}
        suggestedText={suggestedText}
        onTextUsed={() => setSuggestedText("")}
      />
    </div>
  );
}
