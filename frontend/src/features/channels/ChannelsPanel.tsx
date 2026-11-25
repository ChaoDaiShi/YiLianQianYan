import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Plus, RefreshCw } from "lucide-react";
import { useSearchParams } from "react-router-dom";
import Button from "../../components/ui/Button";
import Modal from "../../components/ui/Modal";
import SoftwareMountShell, { type PlatformContent } from "../software/SoftwareMountShell";
import {
  checkChannel, deleteChannel, listChannelPresets, listChannelProviders, listChannels, setChannelEnabled,
  setChannelReplyMode, startChannelLogin, type ChannelCheckItem, type ChannelLogin, type ChannelPreset,
  type ChannelProvider, type ExternalChannel,
} from "../../api/channels";
import ChannelEditor from "./ChannelEditor";
import ChannelActionMenu from "./ChannelActionMenu";
import ChannelLoginDialog from "./ChannelLoginDialog";
import ChannelPresetsPanel, { clearChannelPresetDraft, hasUnsavedChannelPresetDraft } from "./ChannelPresetsPanel";
import ChannelSettingsDrawer from "./ChannelSettingsDrawer";
import ChannelWorkspace, { type ChannelWorkspaceSection } from "./ChannelWorkspace";
import { channelPlatforms, channelStatusLabel, replyModeLabel, type ChannelPlatform } from "./channelsModel";
import "./channels.css";

type PageView = "workspace" | "presets";
type ViewTarget = { view: PageView; platform?: ChannelPlatform };
const platformForProvider = (provider?: string) => channelPlatforms.find(platform => provider && platform.providers.includes(provider));
let retainedPageView: PageView = "workspace";
const problem = (value: unknown) => value instanceof Error ? value.message : "渠道操作失败";

export default function ChannelsPanel() {
  const [searchParams, setSearchParams] = useSearchParams();
  const queryChannelId = searchParams.get("channel") ?? "";
  const queryRunId = searchParams.get("run") ?? "";
  const queryPlatform = searchParams.get("platform");
  const [providers, setProviders] = useState<ChannelProvider[]>([]);
  const [channels, setChannels] = useState<ExternalChannel[]>([]);
  const [presets, setPresets] = useState<ChannelPreset[]>([]);
  const [selectedId, setSelectedId] = useState(queryChannelId);
  const [section, setSection] = useState<ChannelWorkspaceSection>(queryRunId ? "activity" : "conversations");
  const [view, setViewState] = useState<PageView>(() => queryChannelId || queryRunId ? "workspace" : retainedPageView);
  const [revision, setRevision] = useState(0);
  const [policyRevision, setPolicyRevision] = useState(0);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [actionError, setActionError] = useState("");
  const [presetDirty, setPresetDirty] = useState(() => hasUnsavedChannelPresetDraft());
  const [presetBusy, setPresetBusy] = useState(false);
  const [pendingView, setPendingView] = useState<ViewTarget | null>(null);
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
  const linkedChannel = channels.find(channel => channel.id === queryChannelId);
  const currentChannel = channels.find(channel => channel.id === selectedId) ?? channels[0];
  const platform = platformForProvider(linkedChannel?.provider)
    ?? channelPlatforms.find(item => item.id === queryPlatform)
    ?? platformForProvider(currentChannel?.provider)
    ?? channelPlatforms[0];
  const platformChannels = channels.filter(channel => platform.providers.includes(channel.provider));
  const selectedChannel = platformChannels.find(channel => channel.id === selectedId) ?? platformChannels[0] ?? null;
  const selectedProvider = selectedChannel ? orderedProviders.find(provider => provider.id === selectedChannel.provider) : undefined;

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

  const commitView = useCallback((target: ViewTarget) => {
    setView(target.view);
    if (target.platform) {
      setSelectedId("");
      setSection("conversations");
      setSettingsOpen(false);
      setSearchParams({ platform: target.platform });
    }
  }, [setView, setSearchParams]);

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
        if (queryRunId) setSection(deepLinked?.provider === "onebot11" ? "activity" : "conversations");
      })
      .catch(reason => { if (active) setLoadError(problem(reason)); })
      .finally(() => { if (active) { setLoading(false); setRefreshing(false); } });
    return () => { active = false; };
  }, [revision, queryChannelId, queryRunId]);

  useEffect(() => {
    if (pendingView && !presetDirty && !presetBusy) {
      commitView(pendingView);
      setPendingView(null);
    }
  }, [pendingView, presetDirty, presetBusy, commitView]);

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

  function requestView(next: PageView, nextPlatform?: ChannelPlatform) {
    const target = { view: next, platform: nextPlatform };
    if (view === "presets" && presetDirty && next !== view) {
      setPendingView(target);
      return;
    }
    commitView(target);
  }

  function selectChannel(id: string) {
    setSelectedId(id);
    setSection("conversations");
    setSettingsOpen(false);
    setSearchParams({ channel: id }, { replace: true });
  }

  function selectPlatform(id: ChannelPlatform, closeNavigation?: () => void) {
    if (view === "presets") closeNavigation?.();
    if (id !== platform.id || view !== "workspace") requestView("workspace", id === platform.id ? undefined : id);
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
      setSearchParams({ platform: platform.id }, { replace: true });
      refresh();
    } catch (reason) {
      if (alive.current) { setActionError(problem(reason)); setDeleteTarget(null); }
    } finally { if (alive.current) setBusyChannelId(""); }
  }

  function addChannel(providerId = orderedProviders.find(provider => provider.supported && platform.providers.includes(provider.id))?.id ?? "") {
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
    setSearchParams({ channel: channelId }, { replace: true });
    refresh();
  }

  function editSelectedChannel() {
    if (!selectedChannel) return;
    setEditorChannel(selectedChannel);
    setInitialProvider(selectedChannel.provider);
    setSettingsOpen(false);
    setEditorOpen(true);
  }

  const pageTitle = view === "presets" ? "回复预设" : "消息工作台";
  const showOnboarding = view === "workspace" && !loading && !loadError && platformChannels.length === 0;
  const showWorkspace = view === "workspace" && platformChannels.length > 0;
  const connectionError = selectedChannel?.status === "error" ? selectedChannel.last_error : "";
  const pageError = loadError || (settingsOpen ? "" : actionError || connectionError);

  function renderWorkspace(content: ReactNode, navigation: PlatformContent) {
    const platformContent: PlatformContent | undefined = showWorkspace ? navigation : showOnboarding ? closeNavigation => (
      <Button className="channel-platform-add" size="sm" variant="secondary" onClick={() => { closeNavigation?.(); addChannel(); }}><Plus size={15} />添加渠道</Button>
    ) : undefined;
    return <SoftwareMountShell platform={platform.id} platformContent={platformContent} onPlatformChange={selectPlatform} description={view === "presets" ? pageTitle : undefined}
      actions={!showWorkspace && <>
        <Button variant="ghost" size="sm" iconOnly disabled={refreshing} loading={refreshing} aria-label="刷新消息工作台" title="刷新消息工作台" onClick={refresh}><RefreshCw size={16} /></Button>
        {view === "presets" && <Button variant="secondary" size="sm" onClick={() => requestView("workspace")}>返回消息</Button>}
      </>}>
      <section className="channels-page channels-workspace-page">
        <h2 className="sr-only">{pageTitle}</h2>
        {pageError && <p className="channels-error channels-action-error" role="alert">{pageError}</p>}
        {loading && !showWorkspace && <p className="channels-page-loading" role="status">正在读取渠道配置…</p>}
        {!loading && view === "presets" && !loadError && <div className="channels-presets-workspace"><ChannelPresetsPanel presets={presets} onPresetsChange={setPresets} onUnsavedChange={onPresetUnsavedChange} /></div>}
        {showWorkspace && content}
        {showOnboarding && <div className="channel-workspace-onboarding"><h2>{channels.length ? `还没有${platform.label}渠道` : "从一个消息账号开始"}</h2><p>从左侧添加账号后，可在这里查看会话和审阅回复。</p></div>}
      </section>
    </SoftwareMountShell>;
  }

  return <>
    <ChannelWorkspace
      channels={platformChannels}
      presets={presets}
      channel={showWorkspace ? selectedChannel : null}
      section={selectedChannel?.provider === "onebot11" ? section : "conversations"}
      initialRunId={queryRunId}
      policyRevision={policyRevision}
      revision={revision}
      onSelectChannel={selectChannel}
      onSectionChange={setSection}
      accountAction={closeNavigation => selectedChannel && <ChannelActionMenu busy={refreshing}
        summary={`${selectedChannel.name} · ${channelStatusLabel(selectedChannel.status)} · ${replyModeLabel(selectedChannel.reply_mode)}`}
        actions={[
          ...(selectedChannel.provider === "onebot11" ? [
            ...(section !== "conversations" ? [{ label: "返回会话", run: () => setSection("conversations") }] : []),
            { label: "通讯录", run: () => setSection("contacts") },
            { label: "机器人活动", run: () => setSection("activity") },
          ] : []),
          { label: "设置", run: () => setSettingsOpen(true) },
          { label: "回复预设", run: () => requestView("presets") },
          { label: "添加渠道", run: () => addChannel() },
          { label: "刷新消息工作台", run: refresh },
        ].map(({ label, run }) => ({ label, run: () => { closeNavigation?.(); run(); } }))} />}
      render={renderWorkspace}
    />
    {view === "workspace" && selectedChannel && <ChannelSettingsDrawer
      channel={selectedChannel}
      provider={selectedProvider}
      open={settingsOpen}
      actionError={actionError}
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
      <><Button variant="secondary" disabled={presetBusy} onClick={() => setPendingView(null)}>继续编辑</Button><Button variant="danger" disabled={presetBusy} onClick={() => { if (pendingView) { clearChannelPresetDraft(); setPresetDirty(false); commitView(pendingView); setPendingView(null); } }}>放弃修改并离开</Button></>
    )}>
      <p className="channel-confirm-copy">当前回复预设的修改尚未保存。切换页面会放弃这些更改。</p>
    </Modal>
  </>;
}

function providerOrder(provider: string) {
  const order = channelPlatforms.flatMap(platform => platform.providers);
  const index = order.indexOf(provider);
  return index < 0 ? 9 : index;
}
