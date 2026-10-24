import { useCallback, useEffect, useRef, useState } from "react";
import { Bot, MessageSquareText, Search, UsersRound } from "lucide-react";
import { listChannelSessions, type ChannelMessage, type ChannelProvider, type ChannelSession, type ExternalChannel, type ChannelPreset } from "../../api/channels";
import Button from "../../components/ui/Button";
import ChannelInbox from "./ChannelInbox";
import ChannelDirectory from "./ChannelDirectory";
import ChannelRobotPanel from "./ChannelRobotPanel";

export type ChannelWorkspaceSection = "conversations" | "contacts" | "activity";

const sectionCopy: Record<ChannelWorkspaceSection, string> = {
  conversations: "会话",
  contacts: "通讯录",
  activity: "机器人活动",
};

export default function ChannelWorkspace({
  channels,
  providers,
  presets,
  channel,
  section,
  initialRunId,
  policyRevision,
  revision,
  onSelectChannel,
  onSectionChange,
}: {
  channels: ExternalChannel[];
  providers: ChannelProvider[];
  presets: ChannelPreset[];
  channel: ExternalChannel | null;
  section: ChannelWorkspaceSection;
  initialRunId: string;
  policyRevision: number;
  revision: number;
  onSelectChannel: (id: string) => void;
  onSectionChange: (section: ChannelWorkspaceSection) => void;
}) {
  const [sessions, setSessions] = useState<ChannelSession[]>([]);
  const [sessionsLoading, setSessionsLoading] = useState(true);
  const sessionsRef = useRef<ChannelSession[]>([]);
  const [messages, setMessages] = useState<ChannelMessage[]>([]);
  const [selectedConversationId, setSelectedConversationId] = useState("");
  const [sessionQuery, setSessionQuery] = useState("");
  const [sessionError, setSessionError] = useState("");
  const sessionRequestSequence = useRef(0);
  const sessionRequestInFlight = useRef<{ channelId: string; promise: Promise<void> } | null>(null);
  const channelIdRef = useRef(channel?.id ?? "");
  channelIdRef.current = channel?.id ?? "";

  const channelId = channel?.id ?? "";
  const refreshSessions = useCallback(() => {
    if (!channelId) { setSessionsLoading(false); return Promise.resolve(); }
    if (sessionRequestInFlight.current?.channelId === channelId) return sessionRequestInFlight.current.promise;
    const sequence = ++sessionRequestSequence.current;
    setSessionError("");
    setSessionsLoading(true);
    const promise = listChannelSessions(channelId)
      .then(result => {
        if (sequence !== sessionRequestSequence.current || channelIdRef.current !== channelId) return;
        sessionsRef.current = result.sessions;
        setSessions(result.sessions);
        setSelectedConversationId(current => result.sessions.some(item => item.conversation_id === current)
          ? current : result.sessions[0]?.conversation_id ?? "");
      })
      .catch(reason => {
        if (sequence === sessionRequestSequence.current && channelIdRef.current === channelId) {
          setSessionError(reason instanceof Error ? reason.message : "会话列表无法加载");
        }
      })
      .finally(() => {
        if (sequence === sessionRequestSequence.current && channelIdRef.current === channelId) setSessionsLoading(false);
      })
      .finally(() => {
        if (sessionRequestInFlight.current?.promise === promise) sessionRequestInFlight.current = null;
      });
    sessionRequestInFlight.current = { channelId, promise };
    return promise;
  }, [channelId]);

  useEffect(() => {
    sessionsRef.current = [];
    setSessions([]);
    setSessionsLoading(true);
    setMessages([]);
    setSelectedConversationId("");
    setSessionQuery("");
    setSessionError("");
  }, [channel?.id]);

  useEffect(() => { void refreshSessions(); }, [channelId, revision, refreshSessions]);

  const onMessagesChange = useCallback((items: ChannelMessage[]) => {
    setMessages(items);
    const knownConversations = new Set(sessionsRef.current.map(session => session.conversation_id));
    if (items.some(message => !knownConversations.has(message.conversation_id))) void refreshSessions();
  }, [refreshSessions]);
  const filteredSessions = sessions.filter(session => {
    const latest = messages.filter(message => message.conversation_id === session.conversation_id).sort((a, b) => b.created_at - a.created_at)[0];
    const label = latest?.peer_type === "group" ? "群 " + session.peer_id : latest?.sender_id ?? session.peer_id;
    return (label + " " + session.peer_id).toLocaleLowerCase().includes(sessionQuery.trim().toLocaleLowerCase());
  });
  const selectedSession = sessions.find(item => item.conversation_id === selectedConversationId) ?? null;
  const provider = channel ? providers.find(item => item.id === channel.provider) : undefined;

  return <div className="channels-workspace-shell">
    <aside className="channel-left-rail" aria-label="渠道与导航">
      <div className="channel-left-rail-heading"><span>应用与账号</span><small>{channels.length} 个账号</small></div>
      <div className="channel-account-list" aria-label="消息账号">
        {channels.map(item => {
          const itemProvider = providers.find(entry => entry.id === item.provider);
          return <button key={item.id} type="button" aria-pressed={channel?.id === item.id} className={channel?.id === item.id ? "selected" : ""} onClick={() => onSelectChannel(item.id)}>
            <span className="channel-account-mark" data-provider={item.provider}>{item.provider === "onebot11" ? "Q" : item.provider === "wechat_onebot" || item.provider === "wechat_ilink" ? "微" : itemProvider?.label.slice(0, 1) ?? "·"}</span>
            <span><strong>{item.name}</strong><small>{itemProvider?.label ?? item.provider} · {item.enabled ? "启用" : "停用"}</small></span>
            <i aria-label={item.status} data-status={item.status} />
          </button>;
        })}
        {!channels.length && <p className="channel-workspace-muted">尚无消息账号。</p>}
      </div>
      {channel && <nav className="channel-workspace-nav" aria-label="工作区">
        <div className="channel-left-rail-heading"><span>导航</span></div>
        <button type="button" aria-current={section === "conversations" ? "page" : undefined} onClick={() => onSectionChange("conversations")}><MessageSquareText size={15} />会话</button>
        {channel.provider === "onebot11" && <button type="button" aria-current={section === "contacts" ? "page" : undefined} onClick={() => onSectionChange("contacts")}><UsersRound size={15} />通讯录</button>}
        {channel.provider === "onebot11" && <button type="button" aria-current={section === "activity" ? "page" : undefined} onClick={() => onSectionChange("activity")}><Bot size={15} />机器人活动</button>}
      </nav>}
      {channel && section === "conversations" && <section className="channel-session-section" aria-label="会话列表">
        <div className="channel-session-heading"><strong>会话</strong><span>{sessions.length}</span></div>
        <label className="channel-workspace-search"><Search size={14} /><input aria-label="搜索会话" value={sessionQuery} onChange={event => setSessionQuery(event.target.value)} placeholder="搜索会话" /></label>
        {sessionError && <div className="channel-session-error"><p className="channels-error" role="alert">{sessionError}</p><Button variant="ghost" size="sm" disabled={sessionsLoading} loading={sessionsLoading} onClick={() => void refreshSessions()}>重试会话列表</Button></div>}
        {sessionsLoading && <p role="status" className="channel-workspace-muted">正在读取会话…</p>}
        <div className="channel-session-list">
          {filteredSessions.map(session => {
            const related = messages.filter(message => message.conversation_id === session.conversation_id).sort((a, b) => b.created_at - a.created_at);
            const latest = related[0];
            const title = latest?.peer_type === "group" ? "群 " + session.peer_id : latest?.sender_id ?? session.peer_id;
            return <button key={session.id} type="button" aria-pressed={selectedConversationId === session.conversation_id} className={selectedConversationId === session.conversation_id ? "selected" : ""} onClick={() => setSelectedConversationId(session.conversation_id)}>
              <span className="channel-session-avatar">{session.peer_type === "group" ? <UsersRound size={16} /> : <MessageSquareText size={16} />}</span>
              <span className="channel-session-copy"><strong>{title}</strong><small>{latest?.content ?? "尚无已记录的收信内容"}</small><time>{latest ? new Date(latest.created_at).toLocaleTimeString() : new Date(session.created_at).toLocaleTimeString()}</time></span>
            </button>;
          })}
          {!sessionsLoading && !filteredSessions.length && !sessionError && <p className="channel-workspace-muted">没有已保存会话，仍会读取新消息。</p>}
        </div>
      </section>}
      {channel && section === "contacts" && <p className="channel-rail-note">目录与历史由 QQ OneBot 查询；微信接入不会复用这些工具。</p>}
      {channel && section === "activity" && <p className="channel-rail-note">运行步骤显示在中央详情区。</p>}
    </aside>

    <main className="channel-workspace-main">
      {channel ? <header className="channel-main-heading">
        <div><h2>{section === "conversations" ? selectedSession ? (selectedSession.peer_type === "group" ? "群 " : "") + selectedSession.peer_id : "消息会话" : sectionCopy[section]}</h2><p>{channel.name} · {section === "conversations" ? "本机已保存的消息记录" : section === "contacts" ? provider?.label + " 远端通讯录" : provider?.label + " 目标运行"}</p></div>
        {section === "conversations" && selectedSession && <span className="channel-session-type">{selectedSession.peer_type === "group" ? "群会话" : "私聊会话"}</span>}
      </header> : null}
      {!channel ? <div className="channel-workspace-empty"><span className="channel-workspace-empty-icon"><MessageSquareText size={18} /></span><h2>添加一个消息账号</h2><p>接入后，会话与平台运行状态会显示在这里。</p></div>
        : section === "conversations"
          ? <ChannelInbox key={channel.id + ":" + (selectedSession?.conversation_id ?? "all")} channel={channel} revision={revision} conversationId={selectedSession?.conversation_id} onMessagesChange={onMessagesChange} />
        : section === "contacts" ? <ChannelDirectory channel={channel} />
          : <ChannelRobotPanel key={channel.id} channel={channel} presets={presets} initialRunId={initialRunId} policyRevision={policyRevision} />}
    </main>
  </div>;
}
