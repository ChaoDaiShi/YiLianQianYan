import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { MessageSquareText, Search } from "lucide-react";
import { listChannelSessions, type ChannelMessage, type ChannelSession, type ExternalChannel, type ChannelPreset } from "../../api/channels";
import { getRobotPolicy, listRobotDirectory, listRobotGroupMembers, type RobotDirectoryItem, type RobotDirectoryKind } from "../../api/channelRobot";
import Button from "../../components/ui/Button";
import Select from "../../components/ui/Select";
import type { PlatformContent } from "../software/SoftwareMountShell";
import ChannelInbox from "./ChannelInbox";
import ChannelPeerAvatar, { qqAvatarUrl } from "./ChannelPeerAvatar";
import ChannelDirectory from "./ChannelDirectory";
import ChannelRobotPanel from "./ChannelRobotPanel";
import { channelStatusLabel } from "./channelsModel";

export type ChannelWorkspaceSection = "conversations" | "contacts" | "activity";

const sectionCopy: Record<ChannelWorkspaceSection, string> = {
  conversations: "会话",
  contacts: "通讯录",
  activity: "机器人活动",
};

export default function ChannelWorkspace({
  channels,
  presets,
  channel,
  section,
  initialRunId,
  policyRevision,
  revision,
  onSelectChannel,
  onSectionChange,
  accountAction,
  render,
}: {
  channels: ExternalChannel[];
  presets: ChannelPreset[];
  channel: ExternalChannel | null;
  section: ChannelWorkspaceSection;
  initialRunId: string;
  policyRevision: number;
  revision: number;
  onSelectChannel: (id: string) => void;
  onSectionChange: (section: ChannelWorkspaceSection) => void;
  accountAction: PlatformContent;
  render: (content: ReactNode, navigation: PlatformContent) => ReactNode;
}) {
  const [sessions, setSessions] = useState<ChannelSession[]>([]);
  const [contacts, setContacts] = useState<RobotDirectoryItem[]>([]);
  const [groupMembers, setGroupMembers] = useState<{ groupId: string; names: Map<string, string> } | null>(null);
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

  const channelSessions = sessions.filter(session => session.channel_id === channelId);
  const hasPrivateSessions = channelSessions.some(session => session.peer_type === "private");
  const hasGroupSessions = channelSessions.some(session => session.peer_type === "group");
  const qqEnabled = channel?.provider === "onebot11" && channel.enabled;
  const accountId = channel?.self_id;
  useEffect(() => {
    setContacts([]);
    if (!qqEnabled || (!hasPrivateSessions && !hasGroupSessions)) return;
    let active = true;
    async function loadContacts() {
      const { policy } = await getRobotPolicy(channelId);
      if (!active || (policy.account_id && policy.account_id !== accountId)) return;
      const kinds: RobotDirectoryKind[] = [];
      if (hasPrivateSessions && policy.read_friends) kinds.push("friends");
      if (hasGroupSessions && policy.read_groups) kinds.push("groups");
      const results = await Promise.allSettled(kinds.map(kind => listRobotDirectory(channelId, kind)));
      if (active) setContacts(results.flatMap(result => result.status === "fulfilled" ? result.value.items : []));
    }
    void loadContacts().catch(() => {});
    return () => { active = false; };
  }, [channelId, accountId, qqEnabled, hasPrivateSessions, hasGroupSessions, revision, policyRevision]);

  const onMessagesChange = useCallback((items: ChannelMessage[]) => {
    setMessages(items);
    const knownConversations = new Set(sessionsRef.current.map(session => session.conversation_id));
    if (items.some(message => !knownConversations.has(message.conversation_id))) void refreshSessions();
  }, [refreshSessions]);
  const latestByConversation = useMemo(() => {
    const latest = new Map<string, ChannelMessage>();
    for (const message of messages) {
      const previous = latest.get(message.conversation_id);
      if (!previous || message.created_at > previous.created_at) latest.set(message.conversation_id, message);
    }
    return latest;
  }, [messages]);
  const contactsByPeer = useMemo(() => new Map(contacts.map(contact => [`${contact.peer_type}:${contact.id}`, contact])), [contacts]);
  const query = sessionQuery.trim().toLocaleLowerCase();
  const sessionRows = channelSessions.map(session => {
    const latest = latestByConversation.get(session.conversation_id);
    const contact = contactsByPeer.get(`${session.peer_type}:${session.peer_id}`);
    const name = contact?.name.trim();
    const label = name && name !== session.peer_id ? name : session.peer_type === "group" ? `群 ${session.peer_id}` : session.peer_id;
    return { session, latest, label, avatar: contact ? qqAvatarUrl(contact.id, contact.peer_type === "group") : undefined };
  });
  const filteredSessions = sessionRows.filter(({ session, label }) => (label + " " + session.peer_id).toLocaleLowerCase().includes(query));
  const selectedPeer = sessionRows.find(item => item.session.conversation_id === selectedConversationId);
  const selectedSession = selectedPeer?.session;
  const qqGroupId = qqEnabled && selectedSession?.peer_type === "group" ? selectedSession.peer_id : undefined;
  const connectionNotice = channel && channel.status !== "connected" && channel.status !== "receiving" ? channelStatusLabel(channel.status) : "";

  useEffect(() => {
    setGroupMembers(null);
    if (!qqGroupId) return;
    let active = true;
    void listRobotGroupMembers(channelId, qqGroupId)
      .then(({ items }) => {
        if (active) setGroupMembers({ groupId: qqGroupId, names: new Map(items.map(item => [item.id, item.name])) });
      })
      .catch(() => {});
    return () => { active = false; };
  }, [channelId, accountId, qqGroupId, revision, policyRevision]);

  const navigation: PlatformContent = closeNavigation => (
    <section className="channel-platform-content" aria-label="会话与账号">
      {channels.length > 1 && <div className="channel-rail-account">
        <Select size="sm" aria-label="渠道账号" value={channelId} options={channels.map(item => ({ value: item.id, label: item.name }))} onChange={event => onSelectChannel(event.target.value)} />
      </div>}
      {channel && <section className="channel-session-section" aria-label="会话列表">
        <div className="channel-rail-toolbar">
          <label className="channel-workspace-search"><Search size={14} aria-hidden="true" /><input aria-label="搜索会话" value={sessionQuery} onChange={event => setSessionQuery(event.target.value)} placeholder="搜索会话" /></label>
          {accountAction(closeNavigation)}
        </div>
        {sessionError && <div className="channel-session-error"><p className="channels-error" role="alert">{sessionError}</p><Button variant="ghost" size="sm" disabled={sessionsLoading} loading={sessionsLoading} onClick={() => void refreshSessions()}>重试会话列表</Button></div>}
        {sessionsLoading && <p role="status" className="channel-workspace-muted">正在读取会话…</p>}
        {filteredSessions.length > 0 && <div className="channel-session-list">
          {filteredSessions.map(({ session, latest, label, avatar }) => {
            const active = section === "conversations" && selectedConversationId === session.conversation_id;
            return <button key={session.id} type="button" aria-pressed={active} className={active ? "selected" : ""}
              onClick={() => { setSelectedConversationId(session.conversation_id); onSectionChange("conversations"); closeNavigation?.(); }}>
              <ChannelPeerAvatar group={session.peer_type === "group"} src={avatar} />
              <span className="channel-session-copy"><strong title={label}>{label}</strong><time>{new Date(latest?.created_at ?? session.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</time><small>{latest?.content ?? "暂无消息预览"}</small></span>
            </button>;
          })}
        </div>}
        {!sessionsLoading && !filteredSessions.length && !sessionError && <p className="channel-workspace-muted">{query ? "没有匹配的会话" : "等待收到新消息"}</p>}
      </section>}
    </section>
  );

  return render(
    <section className="channel-workspace-main" aria-label="聊天内容">
      {channel ? <header className="channel-main-heading">
        <div className="channel-chat-identity">
          {section === "conversations" && selectedPeer && <ChannelPeerAvatar group={selectedPeer.session.peer_type === "group"} src={selectedPeer.avatar} />}
          <h2 title={section === "conversations" ? selectedPeer?.label : undefined}>{section === "conversations" ? selectedPeer?.label ?? "消息会话" : sectionCopy[section]}</h2>
        </div>
        {connectionNotice && <span className="channel-chat-state" data-status={channel.status}>{connectionNotice}</span>}
        {!connectionNotice && channel.reply_mode === "automatic" && <span className="channel-chat-state">自动回复</span>}
      </header> : null}
      {!channel ? <div className="channel-workspace-empty"><span className="channel-workspace-empty-icon"><MessageSquareText size={18} /></span><h2>添加一个消息账号</h2><p>接入后，会话与平台运行状态会显示在这里。</p></div>
        : section === "conversations"
          ? <ChannelInbox key={channel.id + ":" + (selectedSession?.conversation_id ?? "all")} channel={channel} revision={revision} conversationId={selectedSession?.conversation_id} groupId={qqGroupId} groupMemberNames={groupMembers?.groupId === qqGroupId ? groupMembers?.names : undefined} onMessagesChange={onMessagesChange} />
        : section === "contacts" ? <ChannelDirectory channel={channel} />
          : <ChannelRobotPanel key={channel.id} channel={channel} presets={presets} initialRunId={initialRunId} policyRevision={policyRevision} />}
    </section>,
    navigation,
  );
}
