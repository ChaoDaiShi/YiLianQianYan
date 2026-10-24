import { useCallback, useEffect, useRef, useState } from "react";
import { Check, MessageSquareText, RefreshCw, Send, WandSparkles } from "lucide-react";
import { Link } from "react-router-dom";
import Button from "../../components/ui/Button";
import Modal from "../../components/ui/Modal";
import { approveChannelSend, dismissChannelMessage, listChannelMessages, regenerateChannelMessage, type ChannelMessage, type ExternalChannel } from "../../api/channels";
import { canEditChannelReply, canSendChannelReply, messageStatusLabel, replyModeDescription } from "./channelsModel";

const problem = (value: unknown) => value instanceof Error ? value.message : "收信操作失败";
const confirmedSendStates = new Set<ChannelMessage["status"]>(["sent", "send_failed", "delivery_unknown", "dismissed"]);
const replyDrafts = new Map<string, string>();
function replyDraftKey(channel: ExternalChannel, message: ChannelMessage) {
  return [channel.id, channel.self_id ?? "", message.peer_type, message.peer_id, message.id].join("\u0000");
}

export default function ChannelInbox({ channel, revision, conversationId, onCountsChange, onMessagesChange }: { channel: ExternalChannel; revision: number; conversationId?: string; onCountsChange?: (channelId: string, counts: { all: number; awaiting: number; sent: number }) => void; onMessagesChange?: (messages: ChannelMessage[]) => void }) {
  const [messages, setMessages] = useState<ChannelMessage[]>([]);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState("");
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [confirmation, setConfirmation] = useState<{ message: ChannelMessage; text: string } | null>(null);
  const [uncertainSendIds, setUncertainSendIds] = useState<Set<string>>(() => new Set());
  const [refreshing, setRefreshing] = useState(false);
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
    setRefreshing(true);
    try {
      const result = await listChannelMessages(channel.id);
      if (!alive.current) return;
      setMessages(result.messages);
      onMessagesChange?.(result.messages);
      const relevant = conversationId ? result.messages.filter(message => message.conversation_id === conversationId) : result.messages;
      const newest = [...relevant].sort((a, b) => b.created_at - a.created_at);
      setSelectedId(current => relevant.some(message => message.id === current) ? current : newest[0]?.id ?? "");
      setUncertainSendIds(current => new Set([...current].filter(id => {
        const message = result.messages.find(item => item.id === id);
        return message ? !confirmedSendStates.has(message.status) : true;
      })));
      setError("");
    } catch (reason) {
      if (alive.current) setError(problem(reason));
    } finally {
      if (alive.current) {
        setLoading(false);
        setRefreshing(false);
      }
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

  const pendingCount = visibleMessages.filter(item => item.status === "awaiting_send" || item.status === "send_failed").length;
  const messageCounts = {
    all: visibleMessages.length,
    awaiting: pendingCount,
    sent: visibleMessages.filter(item => item.status === "sent").length,
  };

  useEffect(() => { onCountsChange?.(channel.id, messageCounts); }, [channel.id, messageCounts.all, messageCounts.awaiting, messageCounts.sent, onCountsChange]);

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
    <section className="channel-inbox" aria-labelledby="channel-inbox-heading">
      <div className="channel-inbox-heading">
        <div><p>{replyModeDescription(channel.reply_mode)}</p></div>
        <Button variant="ghost" size="sm" disabled={refreshing} loading={refreshing} onClick={() => void refresh()} aria-label="刷新消息"><RefreshCw size={14} />刷新消息</Button>
      </div>
      <div className="channel-inbox-counts" aria-label="消息计数">
        <span>全部 <strong>{messageCounts.all}</strong></span>
        <span>{channel.reply_mode === "automatic" ? "待处理草稿" : "待审阅"} <strong>{messageCounts.awaiting}</strong></span>
        <span>已发送 <strong>{messageCounts.sent}</strong></span>
      </div>
      {error && <p className="channels-error" role="alert">{error}</p>}
        {loading ? <p role="status" className="channels-muted">正在读取消息…</p> : error && visibleMessages.length === 0 ? null : visibleMessages.length === 0 ? (
        <div className="channel-inbox-empty"><MessageSquarePlaceholder /><h4>等待第一条消息</h4><p>完成接入后，真实消息和回复会出现在这里。</p><small>{channel.reply_mode === "automatic" ? "新私聊生成回复后会直接发送；已有草稿保持原状。" : "回复正文可编辑；逐条审阅并批准后发送到对应联系人。"}</small></div>
      ) : (
        <div className="channel-thread-panel">
          <div className="channel-thread-feed" aria-label="本机消息记录">
            {chronologicalMessages.map(item => <article key={item.id} className="channel-thread-entry">
              <button type="button" className="channel-thread-message channel-thread-message-inbound" aria-pressed={selectedId === item.id} onClick={() => setSelectedId(item.id)}>
                <span>{item.sender_id} · {item.peer_type === "group" ? "群消息" : "私聊消息"}</span>
                <p>{item.content}</p>
                <time dateTime={new Date(item.created_at).toISOString()}>{new Date(item.created_at).toLocaleString()}</time>
              </button>
              {item.draft_reply !== null && <div className="channel-thread-message channel-thread-message-outgoing" data-status={item.status}>
                <span>忆涟千言 · {messageStatusLabel(item.status, item.send_mode)}</span>
                <p>{item.draft_reply}</p>
                {item.status === "sent" && item.reply_message_id && <small>平台回执：{item.reply_message_id}</small>}
              </div>}
            </article>)}
          </div>

          {message && <div className="channel-reply">
            <div className="channel-reply-header"><div><strong>回复给 {message.sender_id} · {message.peer_type === "group" ? "群" : "私聊"}</strong><span>{messageStatusLabel(message.status, message.send_mode)} · {new Date(message.created_at).toLocaleString()}</span></div><Link to={`/chat/${encodeURIComponent(message.conversation_id)}`}>查看本地对话</Link></div>
            {message.draft_reply !== null && <label>回复正文
              <textarea aria-label="回复正文" rows={4} maxLength={6000} value={text} onChange={event => updateReplyDraft(event.target.value)} readOnly={!canEditChannelReply(message.status)} />
            </label>}
            {message.error && <p className="channels-error" role="alert">{message.error}</p>}
            {message.status === "delivery_unknown" && <p className="channel-delivery-warning" role="status">未取得平台送达确认。请在平台核对这条消息；此状态不会提供重发操作。</p>}
            {uncertainSendIds.has(message.id) && message.status !== "delivery_unknown" && <p className="channel-delivery-warning" role="status">上次发送请求的结果尚未确认。请刷新并核对状态，确认服务端状态前不会再次提交。</p>}
            {message.status === "sent" && message.send_mode === "automatic" && <p className="channels-muted">此回复由自动回复策略发送，没有用户批准记录。</p>}
            {message.status === "sent" && message.reply_message_id && <p className="channel-sent"><Check size={14} />发送确认标识：{message.reply_message_id}</p>}
            {message.status === "dismissed" && <p className="channels-muted">此草稿已放弃，不会发送。</p>}
            <div className="channel-reply-actions">
              {message.status === "failed" && <Button variant="secondary" size="sm" disabled={sending} onClick={() => void regenerate()}><WandSparkles size={14} />重新生成草稿</Button>}
              {canSendChannelReply(message.status, channel.enabled) && <Button size="sm" disabled={sending || !text.trim() || uncertainSendIds.has(message.id)} onClick={() => setConfirmation({ message, text })}><Send size={14} />审阅并批准发送</Button>}
              {(message.status === "awaiting_send" || message.status === "send_failed") && <Button variant="ghost" size="sm" disabled={sending} onClick={() => void dismiss()}>放弃草稿</Button>}
            </div>
          </div>}
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
