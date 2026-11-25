import { useCallback, useEffect, useMemo, useState } from "react";
import { listConversationOutputs } from "../../api/conversationOutputs";
import type { Message, ToolCallRecord } from "../../types";
import { MESSAGE_LIST_VIEWPORT_CLASS_NAME } from "../layout/workspaceLayout";
import Modal from "../ui/Modal";
import { presentExecutionError } from "./errorDisplay";
import MessageProcess from "./MessageProcess";
import MessageBubble from "./MessageBubble";
import ReplyImagePreviews, { type ReplyImage } from "./ReplyImagePreviews";
import StreamingText from "./StreamingText";
import { streamingRichMarkdown } from "./RichContent";

export interface StreamingState {
  content: string;
  reasoning?: string;
  toolCalls: ToolCallRecord[];
}

interface MessageListProps {
  conversationId: string | null;
  messages: Message[];
  streaming: StreamingState | null;
  scrollContainerRef: React.RefObject<HTMLDivElement>;
  onScroll?: React.UIEventHandler<HTMLDivElement>;
  error?: string | null;
  onRetry?: () => void;
  retryLabel?: string;
  onLocateMessage?: () => void;
  onSpeak?: (text: string) => void;
  onRollback?: (message: Message) => void;
  onDelete?: (message: Message) => void;
  onBranch?: (message: Message) => void;
  actionsDisabled?: boolean;
  persistedMessageIds?: ReadonlySet<string>;
}

function ErrorNotice({ error, onRetry, onLocateMessage, retryLabel }: { error: string; onRetry?: () => void; onLocateMessage?: () => void; retryLabel?: string }) {
  const presentation = presentExecutionError(error);

  return (
    <section
      className="conversation-error-notice mb-4 rounded-[var(--radius-lg)] border border-[var(--danger-border)] bg-[var(--danger-soft)] px-4 py-3"
      aria-label="任务执行错误"
    >
      <div className="flex items-start gap-3">
        <span className="mt-1 h-2.5 w-2.5 shrink-0 rounded-full bg-[var(--danger)]" />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-[var(--danger-fg)]">
            任务没有成功完成
          </p>
          <p className="mt-1 text-xs leading-5 text-[var(--text-secondary)]">
            {presentation.message}
          </p>
          <div className="mt-3 flex flex-wrap items-center gap-3">
            {onLocateMessage && <button type="button" onClick={onLocateMessage} className="rounded-[var(--radius-sm)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3 py-1.5 text-xs text-[var(--text-primary)]">定位原问题</button>}
            {onRetry && (
              <button
                type="button"
                onClick={onRetry}
                className="rounded-[var(--radius-sm)] bg-[var(--danger)] px-3 py-1.5 text-xs font-medium text-[var(--danger-fg)] transition-colors hover:opacity-90"
              >
                {retryLabel || "重新尝试"}
              </button>
            )}
            <details className="text-xs text-[var(--text-secondary)]">
              <summary className="cursor-pointer select-none hover:text-[var(--text-primary)]">
                查看错误日志
              </summary>
              {presentation.code ? (
                <p className="mt-2 font-mono text-[var(--text-faint)]">
                  错误代码：{presentation.code}
                </p>
              ) : null}
              <pre className="mt-2 max-h-40 max-w-full overflow-auto whitespace-pre-wrap rounded-lg border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2 font-mono text-[var(--text-secondary)]">
                {presentation.technical}
              </pre>
            </details>
          </div>
        </div>
      </div>
    </section>
  );
}

export default function MessageList({
  conversationId,
  messages,
  streaming,
  scrollContainerRef,
  onScroll,
  error,
  onRetry,
  retryLabel,
  onLocateMessage,
  onSpeak,
  onRollback,
  onDelete,
  onBranch,
  actionsDisabled,
  persistedMessageIds,
}: MessageListProps) {
  const [loadedImages, setLoadedImages] = useState<{
    conversationId: string;
    byCall: Map<string, ReplyImage>;
  } | null>(null);
  const [preview, setPreview] = useState<{ title: string; url: string } | null>(null);
  const openImage = useCallback((title: string, blob: Blob) => {
    setPreview({ title, url: URL.createObjectURL(blob) });
  }, []);
  useEffect(() => setPreview(null), [conversationId]);
  useEffect(() => () => {
    if (preview) URL.revokeObjectURL(preview.url);
  }, [preview]);
  const { messageByCall, savedImageCalls } = useMemo(() => {
    const owners = new Map<string, string>();
    const calls = [];
    for (const message of messages) {
      if (message.role !== "assistant") continue;
      for (const tool of message.process?.tools ?? []) {
        if (tool.name === "image_generation") {
          owners.set(tool.toolCallId, message.id);
          calls.push(`${tool.toolCallId}:${tool.execution?.status ?? ""}`);
        }
      }
    }
    return { messageByCall: owners, savedImageCalls: calls.join("\n") };
  }, [messages]);
  const completedImageCalls = streaming?.toolCalls.filter(tool =>
    tool.name === "image_generation" && (tool.status === "success" || tool.status === "error")
  ) ?? [];
  const liveImageCalls = completedImageCalls.map(tool => tool.toolCallId).join("\n");

  useEffect(() => {
    if (!conversationId || (!savedImageCalls && !liveImageCalls)) return;
    const controller = new AbortController();
    void listConversationOutputs(conversationId, controller.signal).then(outputs => {
      if (controller.signal.aborted) return;
      const byCall = new Map<string, ReplyImage>();
      for (const output of outputs) {
        if (output.kind === "image" && output.status === "ready" && output.tool_call_id && output.resource_id) {
          byCall.set(output.tool_call_id, { id: output.id, resourceId: output.resource_id, title: output.title });
        }
      }
      setLoadedImages({ conversationId, byCall });
    }).catch(() => {
      // Conversation replies remain readable if the separate outputs request fails.
    });
    return () => controller.abort();
  }, [conversationId, savedImageCalls, liveImageCalls]);

  const imagesByCall = loadedImages && loadedImages.conversationId === conversationId ? loadedImages.byCall : undefined;
  const imagesByMessage = useMemo(() => {
    const grouped = new Map<string, ReplyImage[]>();
    for (const [callId, messageId] of messageByCall) {
      const image = imagesByCall?.get(callId);
      if (image) {
        const images = grouped.get(messageId) ?? [];
        images.push(image);
        grouped.set(messageId, images);
      }
    }
    return grouped;
  }, [imagesByCall, messageByCall]);
  const streamingImages = completedImageCalls.flatMap(tool => {
    const image = imagesByCall?.get(tool.toolCallId);
    return image && !messageByCall.has(tool.toolCallId) ? [image] : [];
  });

  return (
    <div
      ref={scrollContainerRef}
      onScroll={onScroll}
      className={`${MESSAGE_LIST_VIEWPORT_CLASS_NAME} conversation-message-list`}
    >
      <div className="message-column conversation-message-list-inner">
        {messages.map((message) => (
          <MessageBubble
            key={message.id}
            message={message}
            conversationId={persistedMessageIds && !persistedMessageIds.has(message.id) ? null : conversationId}
            images={imagesByMessage.get(message.id)}
            onOpenImage={openImage}
            onSpeak={onSpeak}
            onRollback={onRollback}
            onDelete={onDelete}
            onBranch={onBranch}
            actionsDisabled={actionsDisabled || Boolean(persistedMessageIds && !persistedMessageIds.has(message.id))}
          />
        ))}

        {streaming && (
          <div className="conversation-streaming-state mb-4 animate-msg-in">
            <MessageProcess streaming process={{ reasoning: streaming.reasoning, tools: streaming.toolCalls.map(record => ({ ...record, execution: record })) }}/>

            {streaming.content && (
              <div className="conversation-streaming-message conversation-message-assistant min-w-0 max-w-[820px] rounded-2xl rounded-tl-md border border-[var(--border-soft)] bg-[var(--surface-solid)] p-4 text-[var(--text-primary)]">
                <StreamingText text={streamingRichMarkdown(streaming.content)} />
              </div>
            )}
            <ReplyImagePreviews images={streamingImages} onOpen={openImage} />

            {!streaming.content && !streaming.reasoning && streaming.toolCalls.length === 0 && (
              <div className="conversation-streaming-wait flex items-center gap-3 px-1 py-3" aria-label="正在生成">
                <span className="text-xs text-[var(--text-secondary)]">
                  正在接收小涟的回复…
                </span>
                <span className="h-2 w-2 animate-pulse rounded-full bg-[var(--accent-primary)]" />
              </div>
            )}
          </div>
        )}

        {error && <ErrorNotice error={error} onRetry={onRetry} onLocateMessage={onLocateMessage} retryLabel={retryLabel} />}
      </div>
      <Modal open={Boolean(preview)} onClose={() => setPreview(null)} title="生成图片预览" size="xl">
        {preview && <img src={preview.url} alt={preview.title} className="mx-auto block max-h-[70dvh] max-w-full object-contain" />}
      </Modal>
    </div>
  );
}
