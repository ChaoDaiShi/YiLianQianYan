import { useState, useEffect, useRef, useCallback } from "react";
import { sendMessage, loadConversation, type AgentEvent } from "../../api/client";
import { Message } from "../../types";
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
    { name: string; args: Record<string, unknown>; status: "running" | "success" | "error"; result?: string }
  >;
}

export default function ChatView({ conversationId, onConversationChange, showSidebar, onToggleSidebar }: ChatViewProps) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [streaming, setStreaming] = useState<StreamingState | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [currentConvId, setCurrentConvId] = useState<string | null>(conversationId);
  const abortRef = useRef<AbortController | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null!);
  const [suggestedText, setSuggestedText] = useState("");

  // Load conversation when ID changes
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

  // Handle SSE events
  const handleAgentEvent = useCallback((event: AgentEvent) => {
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

      case "done":
        setStreaming((prev) => {
          if (prev) {
            // Preserve tool calls so they render below the final message
            const toolCalls: Array<{
              toolCallId: string;
              name: string;
              args: Record<string, unknown>;
              status: "running" | "success" | "error";
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
        console.error("Agent error:", event.error);
        break;
    }
  }, [currentConvId]);

  // Auto-scroll
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streaming]);

  const handleSend = useCallback(
    async (text: string) => {
      if (!text.trim() || isLoading) return;

      const userMsg: Message = {
        id: crypto.randomUUID(),
        role: "user",
        content: text,
        created_at: Date.now(),
      };
      setMessages((prev) => [...prev, userMsg]);
      setIsLoading(true);

      const controller = sendMessage(text, currentConvId, handleAgentEvent);
      abortRef.current = controller;
    },
    [currentConvId, isLoading, handleAgentEvent]
  );

  const handleStop = () => {
    abortRef.current?.abort();
    setIsLoading(false);
    setStreaming(null);
  };

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-3 px-4 py-3 border-b border-gray-200 dark:border-gray-700">
        <button
          onClick={onToggleSidebar}
          className="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
          title={showSidebar ? "收起侧栏" : "展开侧栏"}
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
          </svg>
        </button>
        <img src="/favicon.png" alt="忆涟千言" className="w-7 h-7 rounded-lg object-cover" />
        <h2 className="font-semibold text-lg">忆涟千言</h2>
        <div className="flex items-center gap-2 ml-auto">
          <span className="w-2 h-2 rounded-full bg-green-500" title="后端已连接" />
          <span className="text-xs text-gray-400">API :9420</span>
        </div>
      </div>

      {/* Messages */}
      <MessageList messages={messages} streaming={streaming} messagesEndRef={messagesEndRef} onHint={setSuggestedText} />

      {/* Input */}
      <ChatInput onSend={handleSend} isLoading={isLoading} onStop={handleStop} suggestedText={suggestedText} onTextUsed={() => setSuggestedText("")} />
    </div>
  );
}
