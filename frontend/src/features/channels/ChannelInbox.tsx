import { useCallback, useEffect, useRef, useState } from "react";
import { Check, ChevronDown, MessageSquareText, Send, WandSparkles } from "lucide-react";
import { Link } from "react-router-dom";
import Button from "../../components/ui/Button";
import Modal from "../../components/ui/Modal";
import { approveChannelSend, dismissChannelMessage, listChannelMessages, regenerateChannelMessage, type ChannelMessage, type ExternalChannel } from "../../api/channels";
import ChannelPeerAvatar, { qqAvatarUrl } from "./ChannelPeerAvatar";
import { canEditChannelReply, canSendChannelReply, messageStatusLabel } from "./channelsModel";

const problem = (value: unknown) => value instanceof Error ? value.message : "收信操作失败";
const confirmedSendStates = new Set<ChannelMessage["status"]>(["sent", "send_failed", "delivery_unknown", "dismissed"]);
const replyDrafts = new Map<string, string>();
function replyDraftKey(channel: ExternalChannel, message: ChannelMessage) {
  return [channel.id, channel.self_id ?? "", message.peer_type, message.peer_id, message.id].join("\u0000");
}

export default function ChannelInbox({ channel, revision, conversationId, groupId, groupMemberNames, onMessagesChange }: { channel: ExternalChannel; revision: number; conversationId?: string; groupId?: string; groupMemberNames?: ReadonlyMap<string, string>; onMessagesChange?: (messages: ChannelMessage[]) => void }) {
  const [messages, setMessages] = useState<ChannelMessage[]>([]);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState("");
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [confirmation, setConfirmation] = useState<{ message: ChannelMessage; text: string } | null>(null);
  const [uncertainSendIds, setUncertainSendIds] = useState<Set<string>>(() => new Set());
  const requestInFlight = useRef(false);
  const sendStarted = useRef(false);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);

  const refresh = useCallback(async () => {
    if (requestInFlight.current) return;
    requestInFlight.current = true;
    try {
      const result = await listChannelMessages(channel.id);
      if (!alive.current) return;
      setMessages(result.messages);
      onMessagesChange?.(result.messages);
      const relevant = conversationId ? result.messages.filter(message => message.conversation_id === conversationId) : result.messages;
      setSelectedId(current => relevant.some(message => message.id === current) ? current : "");
      setUncertainSendIds(current => new Set([...current].filter(id => {
        const message = result.messages.find(item => item.id === id);
        return message ? !confirmedSendStates.has(message.status) : true;
      })));
      setError("");
    } catch (reason) {
      if (alive.current) setError(problem(reason));
    } finally {
      if (alive.current) setLoading(false);
      requestInFlight.current = false;
    }
  }, [channel.id, conversationId, onMessagesChange]);

  useEffect(() => { void refresh(); }, [refresh, revision]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh();
    }, 4500);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const visibleMessages = conversationId ? messages.filter(item => item.conversation_id === conversationId) : messages;
  const chronologicalMessages = [...visibleMessages].sort((a, b) => a.created_at - b.created_at);
  const message = visibleMessages.find(item => item.id === selectedId) ?? null;
  useEffect(() => {
    if (!message) { setText(""); return; }
    const key = replyDraftKey(channel, message);
    setText(replyDrafts.get(key) ?? message.draft_reply ?? "");
  }, [channel.id, channel.self_id, message?.id, message?.peer_type, message?.peer_id, message?.draft_reply]);

  function updateReplyDraft(value: string) {
    setText(value);
    if (message) replyDrafts.set(replyDraftKey(channel, message), value);
  }

  async function regenerate() {
    if (!message || message.status !== "failed") return;
    setError("");
    try {
      const result = await regenerateChannelMessage(channel.id, message.id);
      if (!alive.current) return;
      replyDrafts.delete(replyDraftKey(channel, message));
      setMessages(current => current.map(item => item.id === result.message.id ? result.message : item));
    } catch (reason) {
      if (alive.current) setError(problem(reason));
    }
  }

  async function dismiss() {
    if (!message || (message.status !== "awaiting_send" && message.status !== "send_failed")) return;
    setError("");
    try {
      const result = await dismissChannelMessage(channel.id, message.id);
      if (!alive.current) return;
      replyDrafts.delete(replyDraftKey(channel, message));
      setMessages(current => current.map(item => item.id === result.message.id ? result.message : item));
    } catch (reason) {
      if (alive.current) setError(problem(reason));
    }
  }

  async function send() {
    if (!confirmation || sendStarted.current) return;
    sendStarted.current = true;
    setSending(true);
    setError("");
    try {
      const result = await approveChannelSend(channel.id, confirmation.message.id, confirmation.text);
      if (!alive.current) return;
      if (result.message.status === "sent" || result.message.status === "send_failed" || result.message.status === "dismissed") {
        replyDrafts.delete(replyDraftKey(channel, confirmation.message));
      }
      setMessages(current => current.map(item => item.id === result.message.id ? result.message : item));
      if (confirmedSendStates.has(result.message.status)) {
        setUncertainSendIds(current => { const next = new Set(current); next.delete(result.message.id); return next; });
      } else {
        setUncertainSendIds(current => new Set(current).add(result.message.id));
      }
      setConfirmation(null);
    } catch (reason) {
      if (alive.current) {
        setUncertainSendIds(current => new Set(current).add(confirmation.message.id));
        setError(`发送结果尚未确认：${problem(reason)}。请刷新并核对平台状态，再决定下一步。`);
        setConfirmation(null);
        void refresh();
      }
    } finally {
      sendStarted.current = false;
      if (alive.current) setSending(false);
    }
  }

  return (
    <section className="channel-inbox" aria-label="本机收件箱">
      {error && <p className="channels-error" role="alert">{error}</p>}
      {loading ? <p role="status" className="channels-muted">正在读取消息…</p> : error && visibleMessages.length === 0 ? null : visibleMessages.length === 0 ? (
        <div className="channel-inbox-empty"><MessageSquarePlaceholder /><h4>等待第一条消息</h4><p>平台送达的消息和回复会显示在这里。</p></div>
      ) : (
        <div className="channel-thread-feed" aria-label="本机消息记录">
          {chronologicalMessages.map(item => {
            const expanded = selectedId === item.id;
            const editable = canEditChannelReply(item.status);
            const draft = editable ? replyDrafts.get(replyDraftKey(channel, item)) ?? item.draft_reply : item.draft_reply;
            const groupMessage = item.peer_type === "group";
            const memberName = groupMessage && item.peer_id === groupId ? groupMemberNames?.get(item.sender_id) : undefined;
            return <article key={item.id} className="channel-thread-entry">
              <div className="channel-thread-incoming">
                {groupMessage && <ChannelPeerAvatar src={memberName ? qqAvatarUrl(item.sender_id) : undefined} />}
                <div className="channel-thread-message-body">
                  <div className="channel-thread-source">
                    {(groupMessage || !conversationId) && <span>{memberName ?? item.sender_id}</span>}
                    <time dateTime={new Date(item.created_at).toISOString()} title={new Date(item.created_at).toLocaleString()}>{new Date(item.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</time>
                  </div>
                  <p className="channel-thread-content">{item.content}</p>
                </div>
              </div>
              <button type="button" className="channel-thread-message" aria-expanded={expanded} onClick={() => setSelectedId(expanded ? "" : item.id)}>
                {!expanded && draft !== null && <span className="channel-thread-preview">{draft}</span>}
                {(item.status !== "sent" || expanded || draft === null) && <span className="channel-thread-status" data-status={item.status}>{messageStatusLabel(item.status, item.send_mode)}<ChevronDown size={14} aria-hidden="true" /></span>}
              </button>
              {item.error && <p className="channels-error" role="alert">{item.error}</p>}
              {item.status === "delivery_unknown" && <p className="channel-delivery-warning" role="status">未取得平台送达确认，请在平台核对这条消息。</p>}
              {uncertainSendIds.has(item.id) && item.status !== "delivery_unknown" && <p className="channel-delivery-warning" role="status">上次发送请求的结果尚未确认。请刷新并核对平台状态。</p>}
              {expanded && <div className="channel-reply">
                {editable && draft !== null && <label>回复正文
                  <textarea aria-label="回复正文" rows={3} maxLength={6000} value={text} onChange={event => updateReplyDraft(event.target.value)} />
                </label>}
                {!editable && draft !== null && <p className="channel-thread-reply-content">{draft}</p>}
                {item.status === "sent" && item.reply_message_id && <p className="channel-sent"><Check size={14} />发送确认标识：{item.reply_message_id}</p>}
                <div className="channel-reply-actions">
                  {item.status === "failed" && <Button variant="secondary" size="sm" disabled={sending} onClick={() => void regenerate()}><WandSparkles size={14} />重新生成草稿</Button>}
                  {canSendChannelReply(item.status, channel.enabled) && <Button size="sm" disabled={sending || !text.trim() || uncertainSendIds.has(item.id)} onClick={() => setConfirmation({ message: item, text })}><Send size={14} />审阅并批准发送</Button>}
                  {editable && <Button variant="ghost" size="sm" disabled={sending} onClick={() => void dismiss()}>放弃草稿</Button>}
                  <Link className="channel-reply-link" to={`/chat/${encodeURIComponent(item.conversation_id)}`}>查看本地对话</Link>
                </div>
              </div>}
            </article>;
          })}
        </div>
      )}

      <Modal open={Boolean(confirmation)} onClose={() => { if (!sending) setConfirmation(null); }} title="确认发送外部消息" size="md" footer={(
        <>
          <Button variant="secondary" disabled={sending} onClick={() => setConfirmation(null)}>返回编辑</Button>
          <Button disabled={sending || !confirmation?.text.trim()} loading={sending} loadingLabel="发送中…" onClick={() => void send()}>批准并发送此消息</Button>
        </>
      )}>
        {confirmation && <div className="channel-send-review">
          <p><strong>渠道：</strong>{channel.name}</p>
          <p><strong>收件人：</strong>{confirmation.message.sender_id}</p>
          <pre>{confirmation.text}</pre>
          <p className="channels-muted">批准后会向上述联系人发送这份完整正文。若结果无法确认，请先核对平台，不要重复提交。</p>
        </div>}
      </Modal>
    </section>
  );
}

function MessageSquarePlaceholder() {
  return <span className="channel-inbox-empty-icon" aria-hidden="true"><MessageSquareText size={20} /></span>;
}
