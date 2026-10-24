import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Plus, RefreshCw, Settings2 } from "lucide-react";
import { useSearchParams } from "react-router-dom";
import Button from "../../components/ui/Button";
import Modal from "../../components/ui/Modal";
import {
  checkChannel, deleteChannel, listChannelPresets, listChannelProviders, listChannels, setChannelEnabled,
  setChannelReplyMode, startChannelLogin, type ChannelCheckItem, type ChannelLogin, type ChannelPreset,
  type ChannelProvider, type ExternalChannel,
} from "../../api/channels";
import ChannelEditor from "./ChannelEditor";
import ChannelLoginDialog from "./ChannelLoginDialog";
import ChannelPresetsPanel, { clearChannelPresetDraft, hasUnsavedChannelPresetDraft } from "./ChannelPresetsPanel";
import ChannelSettingsDrawer from "./ChannelSettingsDrawer";
import ChannelWorkspace, { type ChannelWorkspaceSection } from "./ChannelWorkspace";
import { channelStatusLabel } from "./channelsModel";
import "./channels.css";

type PageView = "workspace" | "presets";
let retainedPageView: PageView = "workspace";
const problem = (value: unknown) => value instanceof Error ? value.message : "渠道操作失败";

export default function ChannelsPanel() {
  const [searchParams, setSearchParams] = useSearchParams();
  const queryChannelId = searchParams.get("channel") ?? "";
  const queryRunId = searchParams.get("run") ?? "";
  const [providers, setProviders] = useState<ChannelProvider[]>([]);
  const [channels, setChannels] = useState<ExternalChannel[]>([]);
  const [presets, setPresets] = useState<ChannelPreset[]>([]);
  const [selectedId, setSelectedId] = useState(queryChannelId);
  const [section, setSection] = useState<ChannelWorkspaceSection>(queryRunId ? "activity" : "conversations");
  const [view, setViewState] = useState<PageView>(() => retainedPageView);
  const [revision, setRevision] = useState(0);
  const [policyRevision, setPolicyRevision] = useState(0);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [actionError, setActionError] = useState("");
  const [presetDirty, setPresetDirty] = useState(() => hasUnsavedChannelPresetDraft());
  const [presetBusy, setPresetBusy] = useState(false);
  const [pendingView, setPendingView] = useState<PageView | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editorChannel, setEditorChannel] = useState<ExternalChannel | null>(null);
  const [initialProvider, setInitialProvider] = useState("");
  const [loginChannel, setLoginChannel] = useState<ExternalChannel | null>(null);
  const [loginState, setLoginState] = useState<ChannelLogin | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ExternalChannel | null>(null);
  const [busyChannelId, setBusyChannelId] = useState("");
  const [replyBusyId, setReplyBusyId] = useState("");
  const [checkBusyId, setCheckBusyId] = useState("");
  const [checksByChannel, setChecksByChannel] = useState<Record<string, ChannelCheckItem[]>>({});
  const alive = useRef(true);

  const orderedProviders = useMemo(() => [...providers].sort((a, b) => providerOrder(a.id) - providerOrder(b.id)), [providers]);
  const selectedChannel = channels.find(channel => channel.id === selectedId) ?? null;
  const selectedProvider = selectedChannel ? orderedProviders.find(provider => provider.id === selectedChannel.provider) : undefined;
  const selectedConnected = selectedChannel?.status === "connected" || selectedChannel?.status === "receiving";

  const refresh = useCallback(() => {
    setChecksByChannel({});
    setActionError("");
    setRevision(value => value + 1);
  }, []);

  const onPresetUnsavedChange = useCallback((dirty: boolean, busy: boolean) => {
    setPresetDirty(dirty);
    setPresetBusy(busy);
  }, []);

  const setView = useCallback((next: PageView) => {
    retainedPageView = next;
    setViewState(next);
  }, []);

  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setRefreshing(revision > 0);
    setLoadError("");
    void Promise.all([listChannelProviders(), listChannels(), listChannelPresets()])
      .then(([providerResult, channelResult, presetResult]) => {
        if (!active) return;
        setProviders(providerResult.providers);
        setChannels(channelResult.channels);
        setPresets(presetResult.presets);
        const deepLinked = channelResult.channels.find(channel => channel.id === queryChannelId);
        setSelectedId(current => deepLinked?.id ?? (channelResult.channels.some(channel => channel.id === current)
          ? current : channelResult.channels[0]?.id ?? ""));
        if (queryRunId) setSection("activity");
      })
      .catch(reason => { if (active) setLoadError(problem(reason)); })
      .finally(() => { if (active) { setLoading(false); setRefreshing(false); } });
    return () => { active = false; };
  }, [revision, queryChannelId, queryRunId]);

  useEffect(() => {
    if (pendingView && !presetDirty && !presetBusy) {
      setView(pendingView);
      setPendingView(null);
    }
  }, [pendingView, presetDirty, presetBusy]);

  useEffect(() => {
    if (view !== "workspace" || busyChannelId || replyBusyId || checkBusyId) return;
    let active = true;
    let inFlight = false;
    const sync = async () => {
      if (!active || inFlight || document.visibilityState !== "visible") return;
      inFlight = true;
      try {
        const result = await listChannels();
        if (active) setChannels(result.channels);
      } catch (reason) {
        if (active) setActionError("渠道状态更新失败：" + problem(reason));
      } finally { inFlight = false; }
    };
    const timer = window.setInterval(() => void sync(), 4500);
    return () => { active = false; window.clearInterval(timer); };
  }, [view, revision, busyChannelId, replyBusyId, checkBusyId]);

  function requestView(next: PageView) {
    if (view === "presets" && presetDirty && next !== view) {
      setPendingView(next);
      return;
    }
    setView(next);
  }

  function selectChannel(id: string) {
    setSelectedId(id);
    setSection("conversations");
    setSettingsOpen(false);
    setSearchParams({ channel: id }, { replace: true });
  }

  async function changeReplyMode(channel: ExternalChannel) {
    setReplyBusyId(channel.id);
    setActionError("");
    try {
      const result = await setChannelReplyMode(channel.id, channel.reply_mode === "automatic" ? "manual" : "automatic");
      if (alive.current) setChannels(current => current.map(item => item.id === channel.id ? result.channel : item));
    } catch (reason) { if (alive.current) setActionError(problem(reason)); }
    finally { if (alive.current) setReplyBusyId(""); }
  }

  async function toggleChannel(channel: ExternalChannel) {
    setBusyChannelId(channel.id);
    setActionError("");
    try {
      const result = await setChannelEnabled(channel.id, !channel.enabled);
      if (alive.current) setChannels(current => current.map(item => item.id === channel.id ? result.channel : item));
    } catch (reason) { if (alive.current) setActionError(problem(reason)); }
    finally { if (alive.current) setBusyChannelId(""); }
  }

  async function runCheck(channel: ExternalChannel) {
    setCheckBusyId(channel.id);
    setActionError("");
    try {
      const result = await checkChannel(channel.id);
      if (!alive.current) return;
      setChecksByChannel(current => ({ ...current, [channel.id]: result.checks }));
      setChannels(current => current.map(item => item.id === channel.id ? result.channel : item));
    } catch (reason) { if (alive.current) setActionError(problem(reason)); }
    finally { if (alive.current) setCheckBusyId(""); }
  }

  async function beginLogin(channel: ExternalChannel) {
    setBusyChannelId(channel.id);
    setActionError("");
    try {
      const result = await startChannelLogin(channel.id);
      if (!alive.current) return;
      setLoginState(result.login);
      setLoginChannel(channel);
    } catch (reason) { if (alive.current) setActionError(problem(reason)); }
    finally { if (alive.current) setBusyChannelId(""); }
  }

  async function removeChannel() {
    if (!deleteTarget) return;
    setBusyChannelId(deleteTarget.id);
    setActionError("");
    try {
      await deleteChannel(deleteTarget.id);
      if (!alive.current) return;
      setChannels(current => current.filter(item => item.id !== deleteTarget.id));
      setSelectedId(current => current === deleteTarget.id ? "" : current);
      setDeleteTarget(null);
      setSettingsOpen(false);
      refresh();
    } catch (reason) {
      if (alive.current) { setActionError(problem(reason)); setDeleteTarget(null); }
    } finally { if (alive.current) setBusyChannelId(""); }
  }

  function addChannel(providerId = orderedProviders.find(provider => provider.supported)?.id ?? "") {
    setEditorChannel(null);
    setInitialProvider(providerId);
    setEditorOpen(true);
    setActionError("");
  }

  function afterSaved(channelId: string) {
    setEditorOpen(false);
    setEditorChannel(null);
    setSelectedId(channelId);
    setSettingsOpen(false);
    refresh();
  }

  function editSelectedChannel() {
    if (!selectedChannel) return;
    setEditorChannel(selectedChannel);
    setInitialProvider(selectedChannel.provider);
    setSettingsOpen(false);
    setEditorOpen(true);
  }

  const statusText = selectedChannel ? channelStatusLabel(selectedChannel.status) : channels.length ? "选择账号" : "未连接账号";
  const pageTitle = view === "presets" ? "回复预设" : "消息工作台";

  return <section className="channels-page channels-workspace-page">
    <header className="channel-topbar">
      <div className="channel-topbar-title"><h1>{pageTitle}</h1><span>{selectedChannel ? selectedChannel.name + (selectedChannel.self_id ? " · " + selectedChannel.self_id : "") : "本机消息账号"}</span></div>
      <div className="channel-topbar-state" data-connected={selectedConnected}><i aria-hidden="true" />{statusText}<small>{selectedChannel?.enabled ? "已启用" : selectedChannel ? "已停用" : ""}</small></div>
      <div className="channel-topbar-actions">
        {view === "workspace" && selectedChannel && <>
          <Button variant={section === "activity" ? "primary" : "secondary"} onClick={() => setSection("activity")}>机器人</Button>
          <Button variant="ghost" onClick={() => requestView("presets")}>预设</Button>
          <Button variant={selectedChannel.reply_mode === "automatic" ? "secondary" : "ghost"} disabled={replyBusyId === selectedChannel.id} loading={replyBusyId === selectedChannel.id} onClick={() => void changeReplyMode(selectedChannel)}>{selectedChannel.reply_mode === "automatic" ? "自动回复" : "审阅后发送"}</Button>
        </>}
        <Button variant="ghost" disabled={refreshing} loading={refreshing} aria-label="刷新消息工作台" onClick={refresh}><RefreshCw size={15} />刷新</Button>
        {view === "presets" && <Button variant="secondary" onClick={() => requestView("workspace")}>返回消息</Button>}
        {view === "workspace" && selectedChannel && <Button variant="secondary" onClick={() => setSettingsOpen(true)}><Settings2 size={15} />设置</Button>}
        <Button onClick={() => addChannel()}><Plus size={15} />添加渠道</Button>
      </div>
    </header>

    {(loadError || actionError) && <p className="channels-error channels-action-error" role="alert">{loadError || actionError}</p>}
    {loading && <p className="channels-page-loading" role="status">正在读取渠道配置…</p>}
    {!loading && view === "presets" && !loadError && <div className="channels-presets-workspace"><ChannelPresetsPanel presets={presets} onPresetsChange={setPresets} onUnsavedChange={onPresetUnsavedChange} /></div>}
    {!loading && view === "workspace" && !loadError && channels.length > 0 && <ChannelWorkspace
      channels={channels}
      providers={orderedProviders}
      presets={presets}
      channel={selectedChannel}
      section={section}
      initialRunId={queryRunId}
      policyRevision={policyRevision}
      revision={revision}
      onSelectChannel={selectChannel}
      onSectionChange={setSection}
    />}
    {!loading && !loadError && channels.length === 0 && <div className="channel-workspace-onboarding"><h2>从一个消息账号开始</h2><p>连接成功后，平台真实会话和运行状态会显示在这里。</p><Button onClick={() => addChannel()}><Plus size={15} />添加渠道</Button></div>}
    {view === "workspace" && selectedChannel && <ChannelSettingsDrawer
      channel={selectedChannel}
      provider={selectedProvider}
      open={settingsOpen}
      onClose={() => setSettingsOpen(false)}
      onPolicySaved={() => setPolicyRevision(value => value + 1)}
      onEdit={editSelectedChannel}
      onCheck={() => void runCheck(selectedChannel)}
      checkBusy={checkBusyId === selectedChannel.id}
      checks={checksByChannel[selectedChannel.id]}
      onBeginLogin={() => void beginLogin(selectedChannel)}
      loginBusy={busyChannelId === selectedChannel.id}
      onToggleChannel={() => void toggleChannel(selectedChannel)}
      channelBusy={busyChannelId === selectedChannel.id}
      replyBusy={replyBusyId === selectedChannel.id}
      onChangeReplyMode={() => void changeReplyMode(selectedChannel)}
      onDelete={() => setDeleteTarget(selectedChannel)}
    />}
    {editorOpen && <ChannelEditor channel={editorChannel} initialProvider={initialProvider} providers={orderedProviders.filter(provider => provider.supported)} presets={presets} presentation={editorChannel ? "drawer" : "modal"} onClose={() => { setEditorOpen(false); setEditorChannel(null); }} onSaved={afterSaved} />}
    {loginChannel && loginState && <ChannelLoginDialog channel={loginChannel} initialLogin={loginState} onClose={() => { setLoginChannel(null); setLoginState(null); refresh(); }} onUpdated={refresh} />}
    <Modal open={deleteTarget !== null} onClose={() => { if (!busyChannelId) setDeleteTarget(null); }} title="删除消息渠道" variant="danger" size="md" footer={(
      <><Button variant="secondary" disabled={Boolean(busyChannelId)} onClick={() => setDeleteTarget(null)}>取消</Button><Button variant="danger" disabled={Boolean(busyChannelId)} loading={Boolean(busyChannelId)} loadingLabel="删除中…" onClick={() => void removeChannel()}>停止并删除渠道</Button></>
    )}>
      {deleteTarget && <div className="channel-delete-copy"><p>将停止“{deleteTarget.name}”的连接，并删除该渠道配置、凭据、收件箱记录和会话关联。</p><p>已有本地对话与聊天消息会保留。已发送到平台的消息不会被撤回。</p></div>}
    </Modal>
    <Modal open={pendingView !== null} onClose={() => { if (!presetBusy) setPendingView(null); }} title="放弃未保存的预设修改？" size="sm" footer={(
      <><Button variant="secondary" disabled={presetBusy} onClick={() => setPendingView(null)}>继续编辑</Button><Button variant="danger" disabled={presetBusy} onClick={() => { if (pendingView) { clearChannelPresetDraft(); setPresetDirty(false); setView(pendingView); setPendingView(null); } }}>放弃修改并离开</Button></>
    )}>
      <p className="channel-confirm-copy">当前回复预设的修改尚未保存。切换页面会放弃这些更改。</p>
    </Modal>
  </section>;
}

function providerOrder(provider: string) {
  const order = ["onebot11", "wechat_onebot", "wechat_ilink", "feishu"];
  const index = order.indexOf(provider);
  return index < 0 ? 9 : index;
}
