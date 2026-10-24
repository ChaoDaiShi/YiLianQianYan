import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Clock3, Search, Send, UsersRound, UserRound } from "lucide-react";
import Button from "../../components/ui/Button";
import Modal from "../../components/ui/Modal";
import type { ExternalChannel } from "../../api/channels";
import { readRobotHistory, listRobotDirectory, sendRobotMessage, type RobotAction, type RobotDirectoryItem, type RobotDirectoryKind, type RobotPeerType } from "../../api/channelRobot";

const problem = (value: unknown) => value instanceof Error ? value.message : "QQ 联系人操作失败";
const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
interface RobotSendIntent { request_id: string; status: "pending" | "delivery_unknown"; action?: RobotAction }
const robotSendIntents = new Map<string, RobotSendIntent>();

function sendIntentKey(channel: ExternalChannel, peer: RobotDirectoryItem, text: string) {
  return [channel.id, channel.self_id ?? "", peer.peer_type, peer.id, text].join("\u0000");
}

function oneBotText(value: unknown): string {
  if (typeof value === "string") return value;
  if (Array.isArray(value)) {
    return value.map(part => {
      if (!isRecord(part)) return JSON.stringify(part);
      if (part.type === "text" && isRecord(part.data) && typeof part.data.text === "string") return part.data.text;
      if (part.type === "at" && isRecord(part.data)) return `@${String(part.data.qq ?? part.data.user_id ?? "成员")}`;
      return `[${String(part.type ?? "消息段")}]`;
    }).join("");
  }
  return value == null ? "" : JSON.stringify(value);
}

function recordsOf(data: unknown): Record<string, unknown>[] | null {
  if (Array.isArray(data)) return data.filter(isRecord);
  if (isRecord(data) && Array.isArray(data.data)) return data.data.filter(isRecord);
  return null;
}

function messageTime(value: unknown) {
  if (typeof value !== "number") return "平台未返回时间";
  const milliseconds = value < 100_000_000_000 ? value * 1000 : value;
  return new Date(milliseconds).toLocaleString();
}

export default function ChannelDirectory({ channel }: { channel: ExternalChannel }) {
  const [kind, setKind] = useState<RobotDirectoryKind>("friends");
  const [items, setItems] = useState<RobotDirectoryItem[]>([]);
  const [directoryLoading, setDirectoryLoading] = useState(true);
  const [directoryError, setDirectoryError] = useState("");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<RobotDirectoryItem | null>(null);
  const [history, setHistory] = useState<unknown>(null);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState("");
  const [text, setText] = useState("");
  const [confirmation, setConfirmation] = useState<{ item: RobotDirectoryItem; text: string; request_id: string; intent_key: string; account_id: string } | null>(null);
  const [sending, setSending] = useState(false);
  const sendStarted = useRef(false);
  const [sendError, setSendError] = useState("");
  const [lastAction, setLastAction] = useState<RobotAction | null>(null);
  const directoryRequestId = useRef(0);
  const historyRequestId = useRef(0);

  const loadDirectory = useCallback(async () => {
    if (channel.provider !== "onebot11") return;
    const requestId = ++directoryRequestId.current;
    setDirectoryLoading(true);
    setDirectoryError("");
    try {
      const result = await listRobotDirectory(channel.id, kind);
      if (requestId !== directoryRequestId.current) return;
      historyRequestId.current += 1;
      setHistoryLoading(false);
      setHistory(null);
      setHistoryError("");
      setText("");
      setLastAction(null);
      setItems(result.items);
      setSelected(result.items[0] ?? null);
    } catch (reason) {
      if (requestId === directoryRequestId.current) setDirectoryError(problem(reason));
    } finally {
      if (requestId === directoryRequestId.current) setDirectoryLoading(false);
    }
  }, [channel.id, channel.provider, kind]);

  useEffect(() => {
    setItems([]);
    setSelected(null);
    setHistory(null);
    setText("");
    historyRequestId.current += 1;
    void loadDirectory();
    return () => { directoryRequestId.current += 1; };
  }, [loadDirectory]);

  function selectKind(nextKind: RobotDirectoryKind) {
    if (nextKind === kind) return;
    directoryRequestId.current += 1;
    historyRequestId.current += 1;
    setKind(nextKind);
    setItems([]);
    setSelected(null);
    setDirectoryError("");
    setDirectoryLoading(true);
    setHistory(null);
    setHistoryError("");
    setHistoryLoading(false);
    setText("");
    setLastAction(null);
  }

  function selectItem(item: RobotDirectoryItem) {
    historyRequestId.current += 1;
    setSelected(item);
    setHistory(null);
    setHistoryError("");
    setHistoryLoading(false);
    setText("");
    setLastAction(null);
    setSendError("");
  }

  const filteredItems = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return items.filter(item => !needle || `${item.name} ${item.id}`.toLocaleLowerCase().includes(needle));
  }, [items, query]);
  const currentIntentKey = selected && text.trim() ? sendIntentKey(channel, selected, text.trim()) : "";
  const currentIntent = currentIntentKey ? robotSendIntents.get(currentIntentKey) : undefined;

  async function loadHistory() {
    if (!selected) return;
    const requestId = ++historyRequestId.current;
    const selectedKey = selected.peer_type + ":" + selected.id;
    setHistoryLoading(true);
    setHistoryError("");
    setHistory(null);
    try {
      const result = await readRobotHistory(channel.id, { peer_type: selected.peer_type, peer_id: selected.id, count: 20 });
      if (requestId !== historyRequestId.current) return;
      if (!selected || selected.peer_type + ":" + selected.id !== selectedKey) return;
      setHistory(result.data);
    } catch (reason) {
      if (requestId === historyRequestId.current) setHistoryError(problem(reason));
    } finally {
      if (requestId === historyRequestId.current) setHistoryLoading(false);
    }
  }

  async function send() {
    if (!confirmation || sendStarted.current) return;
    sendStarted.current = true;
    setSending(true);
    setSendError("");
    try {
      const result = await sendRobotMessage(channel.id, {
        request_id: confirmation.request_id,
        account_id: confirmation.account_id,
        peer_type: confirmation.item.peer_type,
        peer_id: confirmation.item.id,
        text: confirmation.text,
        approved: true,
      });
      setLastAction(result.action);
      if (result.action.status === "sent" || result.action.status === "send_failed") {
        robotSendIntents.delete(confirmation.intent_key);
      } else if (result.action.status === "delivery_unknown") {
        robotSendIntents.set(confirmation.intent_key, { request_id: confirmation.request_id, status: "delivery_unknown", action: result.action });
      } else {
        robotSendIntents.set(confirmation.intent_key, { request_id: confirmation.request_id, status: "pending", action: result.action });
      }
      setText("");
      setConfirmation(null);
    } catch (reason) {
      if (robotSendIntents.get(confirmation.intent_key)?.status !== "delivery_unknown") {
        robotSendIntents.set(confirmation.intent_key, { request_id: confirmation.request_id, status: "pending" });
      }
      setSendError(problem(reason));
    } finally {
      sendStarted.current = false;
      setSending(false);
    }
  }

  function reviewSend() {
    if (!selected || !channel.self_id || !text.trim()) return;
    const body = text.trim();
    const intent_key = sendIntentKey(channel, selected, body);
    const previous = robotSendIntents.get(intent_key);
    if (previous?.status === "delivery_unknown") {
      setSendError(previous.action?.error ?? "上一条相同目标与正文的发送状态未知。请先在 QQ 核对，不要重复提交。");
      return;
    }
    const request_id = previous?.request_id ?? crypto.randomUUID();
    robotSendIntents.set(intent_key, { request_id, status: "pending" });
    setSendError("");
    setConfirmation({ item: selected, text: body, request_id, intent_key, account_id: channel.self_id });
  }

  if (channel.provider !== "onebot11") {
    return <section className="channel-directory-unavailable" aria-label="通讯录能力说明">
      <span className="channel-workspace-empty-icon"><UsersRound size={19} /></span>
      <h2>{channel.provider === "wechat_onebot" ? "Akasha 微信通讯录尚未接入" : "此渠道暂不提供远端通讯录"}</h2>
      <p>{channel.provider === "wechat_onebot"
        ? "Akasha-WeChat 使用独立反向 WebSocket 收发文本。当前接入不会提供 QQ 式好友、群或历史探索能力。"
        : "这里仅展示该平台已经接入的能力；没有远端目录接口时，不生成联系人示例。"}</p>
    </section>;
  }

  const records = history === null ? null : recordsOf(history);
  return <section className="channel-directory" aria-label="QQ 通讯录">
    <aside className="channel-directory-list">
      <div className="channel-subview-tabs" aria-label="联系人类型">
        <button type="button" aria-pressed={kind === "friends"} onClick={() => selectKind("friends")}><UserRound size={14} />好友</button>
        <button type="button" aria-pressed={kind === "groups"} onClick={() => selectKind("groups")}><UsersRound size={14} />群</button>
        <button type="button" aria-pressed={kind === "recent"} onClick={() => selectKind("recent")}><Clock3 size={14} />最近</button>
      </div>
      <label className="channel-workspace-search"><Search size={14} /><input aria-label="搜索联系人或群" value={query} onChange={event => setQuery(event.target.value)} placeholder="搜索联系人或群" /></label>
      {directoryError && <div className="channel-directory-error" role="alert"><p>{directoryError}</p><Button variant="ghost" size="sm" disabled={directoryLoading} loading={directoryLoading} onClick={() => void loadDirectory()}>重试读取目录</Button></div>}
      <div className="channel-directory-items" aria-label="QQ 远端目录">
        {filteredItems.map(item => <button key={`${item.peer_type}:${item.id}`} type="button" aria-pressed={selected?.id === item.id && selected.peer_type === item.peer_type} onClick={() => selectItem(item)}>
          <span className="channel-directory-avatar">{item.peer_type === "group" ? <UsersRound size={17} /> : <UserRound size={17} />}</span>
          <span className="channel-directory-copy"><strong>{item.name}</strong><small>{item.peer_type === "group" ? "群" : "好友"} · {item.id}</small></span>
        </button>)}
        {directoryLoading && <p role="status" className="channel-workspace-muted">正在读取平台目录…</p>}
        {!directoryLoading && !filteredItems.length && !directoryError && <p className="channel-workspace-muted">平台没有返回符合条件的对象。</p>}
      </div>
    </aside>

    <div className="channel-directory-detail">
      {selected ? <>
        <header className="channel-detail-heading">
          <div><span className="channel-step-label">{selected.peer_type === "group" ? "QQ 群" : "QQ 好友"}</span><h2>{selected.name}</h2><p>平台 ID：<code>{selected.id}</code></p></div>
          <Button variant="secondary" size="sm" disabled={historyLoading} loading={historyLoading} onClick={() => void loadHistory()}>读取最近 20 条</Button>
        </header>
        <section className="channel-remote-history" aria-label="QQ 实际历史">
          <div className="channel-data-source-heading"><h3>QQ 历史</h3><span>OneBot 实际返回 · 与本机收信记录分开</span></div>
          {historyLoading && <p role="status" className="channel-workspace-muted">正在读取平台历史…</p>}
          {historyError && <p role="alert" className="channels-error">{historyError}</p>}
          {history !== null && records && records.length > 0 && <ol>{records.map((record, index) => <li key={`${String(record.message_id ?? "record")}:${index}`}>
            <div><strong>{String(record.sender_name ?? record.nickname ?? record.user_id ?? record.sender_id ?? "平台记录")}</strong><time>{messageTime(record.time)}</time></div>
            <p>{oneBotText(record.message ?? record.content)}</p>
            {record.message_id !== undefined && <small>消息 ID：{String(record.message_id)}</small>}
          </li>)}</ol>}
          {history !== null && records?.length === 0 && <p className="channel-workspace-muted">平台为此对象返回了 0 条历史消息。</p>}
          {history !== null && records === null && <pre className="channel-raw-result">{JSON.stringify(history, null, 2)}</pre>}
          {history === null && !historyLoading && <p className="channel-workspace-muted">选择“读取最近 20 条”后，展示平台返回的历史内容。</p>}
        </section>
        <section className="channel-proactive-composer" aria-label="主动发送消息">
          <label htmlFor="channel-proactive-text">主动发送内容</label>
          <textarea id="channel-proactive-text" aria-label="主动发送内容" rows={3} maxLength={6000} value={text} onChange={event => setText(event.target.value)} placeholder="只会发送给此处显示的对象。" />
          {sendError && <p role="alert" className="channels-error">{sendError}</p>}
          {currentIntent?.status === "delivery_unknown" && <p className="channel-action-receipt channel-action-receipt-delivery_unknown" role="status">{currentIntent.action?.error ?? "相同目标与正文的发送状态未知。请先在 QQ 核对，不要重复提交。"}</p>}
          {lastAction && <p className={`channel-action-receipt channel-action-receipt-${lastAction.status}`} role="status">
            {lastAction.status === "sent" && lastAction.platform_message_id
              ? `平台回执：${lastAction.platform_message_id}`
              : lastAction.status === "delivery_unknown"
                ? "平台未提供消息 ID，送达状态未知。请先在 QQ 核对，不要重复发送。"
                : lastAction.error ?? `发送状态：${lastAction.status}`}
          </p>}
          <div className="channel-proactive-actions"><span>发送前会再次显示账号、目标和完整正文供你审阅。</span><Button disabled={!channel.enabled || !channel.self_id || !selected || !text.trim() || sending || currentIntent?.status === "delivery_unknown"} onClick={reviewSend}><Send size={14} />审阅消息</Button></div>
        </section>
      </> : <div className="channel-workspace-empty"><span className="channel-workspace-empty-icon"><UsersRound size={19} /></span><h2>选择一个好友或群</h2><p>通讯录来自 OneBot 实际查询结果。</p></div>}
    </div>

    <Modal open={Boolean(confirmation)} onClose={() => { if (!sending) setConfirmation(null); }} title="确认发送消息" size="md" footer={(
      <><Button variant="secondary" disabled={sending} onClick={() => setConfirmation(null)}>返回编辑</Button><Button disabled={sending} loading={sending} loadingLabel="提交平台…" onClick={() => void send()}>确认并发送</Button></>
    )}>
      {confirmation && <div className="channel-send-review">{sendError && <p className="channels-error" role="alert">{sendError}</p>}<p><strong>渠道：</strong>{channel.name}</p><p><strong>发信账号：</strong>{confirmation.account_id}</p><p><strong>收件人：</strong>{confirmation.item.name}（{confirmation.item.peer_type === "group" ? "群" : "好友"} · {confirmation.item.id}）</p><pre>{confirmation.text}</pre><p className="channels-muted">批准后只提交这一条消息。若平台未返回 message_id，会保留“送达状态未知”，不会自动重发。</p></div>}
    </Modal>
  </section>;
}

export function peerTypeLabel(peerType: RobotPeerType) {
  return peerType === "group" ? "群" : "私聊";
}
