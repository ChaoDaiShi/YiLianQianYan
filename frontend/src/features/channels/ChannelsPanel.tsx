import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Cable, Check, Clipboard, ExternalLink, Plus, RefreshCw, Settings2, ShieldCheck, Trash2 } from "lucide-react";
import Button from "../../components/ui/Button";
import Modal from "../../components/ui/Modal";
import {
  checkChannel,
  deleteChannel,
  listChannelPresets,
  listChannelProviders,
  listChannels,
  setChannelEnabled,
  setChannelReplyMode,
  startChannelLogin,
  type ChannelCheckItem,
  type ChannelPreset,
  type ChannelProvider,
  type ExternalChannel,
} from "../../api/channels";
import { API_BASE } from "../../core/api/http";
import ChannelEditor from "./ChannelEditor";
import ChannelInbox from "./ChannelInbox";
import ChannelLoginDialog from "./ChannelLoginDialog";
import ChannelPresetsPanel from "./ChannelPresetsPanel";
import { channelReceiveAddress, channelStatusLabel, connectionModeLabel, replyModeDescription, replyModeLabel } from "./channelsModel";
import "./channels.css";

type ChannelView = "inbox" | "presets" | "guide";
type InboxCounts = { all: number; awaiting: number; sent: number };
const problem = (value: unknown) => value instanceof Error ? value.message : "渠道操作失败";
const platformCopy: Record<string, { summary: string; fields: string }> = {
  feishu: { summary: "通过飞书官方长连接或 HTTP 事件回调接收私聊消息。", fields: "App ID · App Secret · 用户范围" },
  onebot11: { summary: "连接你运行的 OneBot 服务，接收 QQ 私聊消息。", fields: "服务地址或 WebSocket · Access Token · QQ 用户" },
  wechat_ilink: { summary: "使用微信扫码授权，在桌面查看登录与收信状态。", fields: "微信扫码 · 登录状态 · 允许联系人" },
};

export default function ChannelsPanel() {
  const [providers, setProviders] = useState<ChannelProvider[]>([]);
  const [channels, setChannels] = useState<ExternalChannel[]>([]);
  const [presets, setPresets] = useState<ChannelPreset[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [revision, setRevision] = useState(0);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [connectionRefreshError, setConnectionRefreshError] = useState("");
  const [actionError, setActionError] = useState("");
  const [view, setView] = useState<ChannelView>("inbox");
  const [hasUnsavedPresetChanges, setHasUnsavedPresetChanges] = useState(false);
  const [presetMutationBusy, setPresetMutationBusy] = useState(false);
  const [pendingView, setPendingView] = useState<ChannelView | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editorChannel, setEditorChannel] = useState<ExternalChannel | null>(null);
  const [initialProvider, setInitialProvider] = useState("");
  const [loginChannel, setLoginChannel] = useState<ExternalChannel | null>(null);
  const [loginState, setLoginState] = useState<import("../../api/channels").ChannelLogin | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ExternalChannel | null>(null);
  const [busyChannelId, setBusyChannelId] = useState("");
  const [replyModeBusyId, setReplyModeBusyId] = useState("");
  const [checkResults, setCheckResults] = useState<Record<string, ChannelCheckItem[]>>({});
  const [checkRunningId, setCheckRunningId] = useState("");
  const [copyState, setCopyState] = useState("");
  const [inboxCounts, setInboxCounts] = useState<Record<string, InboxCounts>>({});
  const alive = useRef(true);

  const selectedChannel = channels.find(channel => channel.id === selectedId) ?? null;
  const orderedProviders = useMemo(() => [...providers].sort((a, b) => providerOrder(a.id) - providerOrder(b.id)), [providers]);
  const connectedCount = channels.filter(channel => channel.status === "connected" || channel.status === "receiving").length;
  const pendingCount = selectedChannel ? inboxCounts[selectedChannel.id]?.awaiting ?? null : 0;

  const refresh = useCallback(() => {
    setCheckResults({});
    setActionError("");
    setRevision(value => value + 1);
  }, []);

  const onPresetUnsavedChange = useCallback((value: boolean, busy: boolean) => {
    setHasUnsavedPresetChanges(value);
    setPresetMutationBusy(busy);
  }, []);

  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);

  function requestView(next: ChannelView) {
    if (view === "presets" && hasUnsavedPresetChanges && next !== view) {
      setPendingView(next);
      return;
    }
    setView(next);
  }

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
        setSelectedId(current => channelResult.channels.some(channel => channel.id === current) ? current : channelResult.channels[0]?.id ?? "");
      })
      .catch(reason => { if (active) setLoadError(problem(reason)); })
      .finally(() => { if (active) { setLoading(false); setRefreshing(false); } });
    return () => { active = false; };
  }, [revision]);

  useEffect(() => {
    if (pendingView && !hasUnsavedPresetChanges && !presetMutationBusy) {
      setView(pendingView);
      setPendingView(null);
    }
  }, [hasUnsavedPresetChanges, pendingView, presetMutationBusy]);

  const recordInboxCounts = useCallback((channelId: string, counts: InboxCounts) => {
    setInboxCounts(current => ({ ...current, [channelId]: counts }));
  }, []);

  useEffect(() => {
    if (view !== "inbox" || busyChannelId || replyModeBusyId || checkRunningId) return;
    let active = true;
    let inFlight = false;
    const sync = async () => {
      if (!active || inFlight || document.visibilityState !== "visible") return;
      inFlight = true;
      try {
        const result = await listChannels();
        if (active) { setChannels(result.channels); setConnectionRefreshError(""); }
      } catch (reason) {
        if (active) setConnectionRefreshError(`渠道状态更新失败：${problem(reason)}`);
      } finally { inFlight = false; }
    };
    const timer = window.setInterval(() => void sync(), 4500);
    return () => { active = false; window.clearInterval(timer); };
  }, [view, revision, busyChannelId, replyModeBusyId, checkRunningId]);

  function addChannel(providerId = orderedProviders.find(provider => provider.supported)?.id ?? "") {
    setEditorChannel(null);
    setInitialProvider(providerId);
    setEditorOpen(true);
    setActionError("");
  }

  async function toggleChannel(channel: ExternalChannel) {
    setBusyChannelId(channel.id);
    setActionError("");
    try {
      await setChannelEnabled(channel.id, !channel.enabled);
      if (alive.current) refresh();
    } catch (reason) {
      if (alive.current) setActionError(problem(reason));
    } finally {
      if (alive.current) setBusyChannelId("");
    }
  }

  async function changeReplyMode(channel: ExternalChannel) {
    const reply_mode = channel.reply_mode === "automatic" ? "manual" : "automatic";
    setReplyModeBusyId(channel.id);
    setActionError("");
    try {
      const result = await setChannelReplyMode(channel.id, reply_mode);
      if (alive.current) setChannels(current => current.map(item => item.id === channel.id ? result.channel : item));
    } catch (reason) {
      if (alive.current) setActionError(problem(reason));
    } finally {
      if (alive.current) setReplyModeBusyId("");
    }
  }

  async function runCheck(channel: ExternalChannel) {
    setCheckRunningId(channel.id);
    setActionError("");
    try {
      const result = await checkChannel(channel.id);
      if (!alive.current) return;
      setCheckResults(current => ({ ...current, [channel.id]: result.checks }));
      setChannels(current => current.map(item => item.id === channel.id ? result.channel : item));
    } catch (reason) {
      if (alive.current) setActionError(problem(reason));
    } finally {
      if (alive.current) setCheckRunningId("");
    }
  }

  async function beginLogin(channel: ExternalChannel) {
    setBusyChannelId(channel.id);
    setActionError("");
    try {
      const result = await startChannelLogin(channel.id);
      if (!alive.current) return;
      setLoginState(result.login);
      setLoginChannel(channel);
      refresh();
    } catch (reason) {
      if (alive.current) setActionError(problem(reason));
    } finally {
      if (alive.current) setBusyChannelId("");
    }
  }

  async function removeChannel() {
    if (!deleteTarget) return;
    setBusyChannelId(deleteTarget.id);
    setActionError("");
    try {
      await deleteChannel(deleteTarget.id);
      if (!alive.current) return;
      setChannels(current => current.filter(item => item.id !== deleteTarget.id));
      setInboxCounts(current => { const next = { ...current }; delete next[deleteTarget.id]; return next; });
      setCheckResults(current => { const next = { ...current }; delete next[deleteTarget.id]; return next; });
      setSelectedId(current => current === deleteTarget.id ? "" : current);
      setDeleteTarget(null);
    } catch (reason) {
      if (alive.current) {
        setActionError(problem(reason));
        setDeleteTarget(null);
      }
    } finally {
      if (alive.current) setBusyChannelId("");
    }
  }

  async function copyConnectionTarget(channel: ExternalChannel) {
    setCopyState("");
    try {
      await navigator.clipboard.writeText(channelReceiveAddress(channel.provider, channel.connection_mode, channel.callback_path, API_BASE) ?? channel.callback_path);
      if (alive.current) setCopyState(channel.id);
    } catch (reason) {
      if (alive.current) setActionError(`复制连接地址失败：${problem(reason)}`);
    }
  }

  function afterSaved(channelId: string) {
    setEditorOpen(false);
    setEditorChannel(null);
    setSelectedId(channelId);
    refresh();
  }

  return (
    <section className="channels-page">
      <header className="channels-page-heading">
        <div><h1>{view === "presets" ? "回复预设" : view === "guide" ? "接入指南" : "消息渠道"}</h1><p>{view === "presets" ? "把喜欢的角色与表达方式保存下来，随时用于不同渠道。" : view === "guide" ? "了解每种协议实际验证的连接能力。" : "把熟悉的聊天入口，接到忆涟千言。"}</p></div>
        <div className="channels-page-actions">
          {view === "inbox" ? <>
            <Button variant="secondary" onClick={() => requestView("presets")}><Settings2 size={15} />管理回复预设</Button>
            <Button onClick={() => addChannel()}><Plus size={15} />添加渠道</Button>
          </> : <Button variant="secondary" onClick={() => requestView("inbox")}>返回渠道</Button>}
        </div>
      </header>

      <nav className="channels-tabs" aria-label="消息渠道页面">
        <button type="button" aria-current={view === "inbox" ? "page" : undefined} onClick={() => requestView("inbox")}>渠道与收信</button>
        <button type="button" aria-current={view === "presets" ? "page" : undefined} onClick={() => requestView("presets")}>回复预设</button>
        <button type="button" aria-current={view === "guide" ? "page" : undefined} onClick={() => requestView("guide")}>接入指南</button>
      </nav>

      {loadError && <p className="channels-error" role="alert">渠道数据无法加载：{loadError}</p>}
      {connectionRefreshError && <p className="channels-error" role="alert">{connectionRefreshError}</p>}
      {actionError && <p className="channels-error channels-action-error" role="alert">{actionError}</p>}

      {loading && <p className="channels-page-loading" role="status">正在读取渠道配置…</p>}
      {!loading && view === "presets" && !loadError && <ChannelPresetsPanel presets={presets} onPresetsChange={setPresets} onUnsavedChange={onPresetUnsavedChange} />}
      {!loading && view === "guide" && !loadError && <ChannelGuide providers={orderedProviders} />}
      {!loading && view === "inbox" && !loadError && <>
        <div className="channels-onboarding">
          <div><h2>从一个渠道开始，随时在桌面接手</h2><p>选择接入模板 → 完成授权 → 选择回复预设 → 收信与审阅</p></div>
          <p className="channels-onboarding-count"><strong>{connectedCount}</strong> 已连接 <span>·</span> <strong>{pendingCount === null ? "…" : pendingCount}</strong> {selectedChannel?.reply_mode === "automatic" ? "已有待审阅草稿" : "待审阅草稿"}{selectedChannel && <small>（{selectedChannel.name}）</small>}</p>
        </div>

        <div className="channel-provider-cards">
          {orderedProviders.map(provider => {
            const providerChannels = channels.filter(channel => channel.provider === provider.id);
            const copy = platformCopy[provider.id] ?? { summary: provider.protocol, fields: provider.limitations.join(" · ") };
            return <article className="channel-provider-card" key={provider.id}>
              <div className="channel-provider-card-title"><h3>{provider.label}</h3><span data-status={providerChannels.length ? "configured" : "empty"}>{providerChannels.length ? `${providerChannels.length} 个渠道` : "未接入"}</span></div>
              <p>{copy.summary}</p>
              <small>{copy.fields}</small>
              {providerChannels.length > 0 && <div className="channel-provider-entries">{providerChannels.map(channel => <button key={channel.id} type="button" onClick={() => { setSelectedId(channel.id); setView("inbox"); }} aria-pressed={selectedChannel?.id === channel.id}>
                <span><strong>{channel.name}</strong><small data-status={channel.status}>{channelStatusLabel(channel.status)}</small><small className="channel-provider-reply-mode">{replyModeLabel(channel.reply_mode)}</small></span><span>{channel.enabled ? "运行中" : "已停用"}</span>
              </button>)}</div>}
              <Button variant={providerChannels.length ? "secondary" : "primary"} size="sm" disabled={!provider.supported} onClick={() => providerChannels.length ? (setSelectedId(providerChannels[0].id), setView("inbox")) : addChannel(provider.id)}>
                {provider.supported ? providerChannels.length ? "查看渠道" : `配置${provider.label.replace("自建应用", "")}` : "暂未支持"}
              </Button>
              {!provider.supported && <small className="channel-provider-unavailable">{provider.limitations.join(" · ")}</small>}
            </article>;
          })}
        </div>

        <div className="channels-workspace-grid">
          <section className="channels-inbox-card">
            {selectedChannel ? <>
              <div className="channels-workspace-card-heading"><div><h2 id="channel-inbox-heading">收信与回复</h2><p>{selectedChannel.name} · {selectedChannel.enabled ? "已启用" : "已停用"} · {replyModeLabel(selectedChannel.reply_mode)}</p></div><Button variant="ghost" size="sm" disabled={refreshing} loading={refreshing} onClick={refresh} aria-label="刷新渠道"><RefreshCw size={14} />刷新</Button></div>
              <ChannelInbox key={selectedChannel.id} channel={selectedChannel} revision={revision} onCountsChange={recordInboxCounts} />
            </> : <div className="channel-inbox-empty"><span className="channel-inbox-empty-icon"><Cable size={22} /></span><h2>等待第一条消息</h2><p>完成接入后，真实消息和回复会出现在这里。</p><small>新消息会按渠道设置的回复方式处理。</small></div>}
          </section>

          {selectedChannel ? <section className="channel-management-card">
            <div className="channel-management-heading"><div><h2>{selectedChannel.name}</h2><p>{connectionModeLabel(selectedChannel.connection_mode)}</p></div><span className="channel-status-pill" data-status={selectedChannel.status}>{channelStatusLabel(selectedChannel.status)}</span></div>
            <div className="channel-management-actions">
              <Button variant="secondary" size="sm" disabled={busyChannelId === selectedChannel.id} onClick={() => { setEditorChannel(selectedChannel); setInitialProvider(selectedChannel.provider); setEditorOpen(true); }}><Settings2 size={14} />编辑配置</Button>
              <Button variant="secondary" size="sm" disabled={replyModeBusyId === selectedChannel.id} loading={replyModeBusyId === selectedChannel.id} loadingLabel="更新中…" onClick={() => void changeReplyMode(selectedChannel)}>{selectedChannel.reply_mode === "automatic" ? "改为审阅发送" : "开启自动回复"}</Button>
              {selectedChannel.provider === "wechat_ilink" && <Button variant="secondary" size="sm" disabled={busyChannelId === selectedChannel.id} loading={busyChannelId === selectedChannel.id} loadingLabel="请求二维码…" onClick={() => void beginLogin(selectedChannel)}>开始扫码</Button>}
              <Button variant="secondary" size="sm" disabled={checkRunningId === selectedChannel.id} loading={checkRunningId === selectedChannel.id} loadingLabel="检查中…" onClick={() => void runCheck(selectedChannel)}>检查连接</Button>
              <Button variant="ghost" size="sm" disabled={busyChannelId === selectedChannel.id} loading={busyChannelId === selectedChannel.id} onClick={() => void toggleChannel(selectedChannel)}>{selectedChannel.enabled ? "停用" : "启用"}</Button>
              <Button variant="ghost" size="sm" aria-label={`删除 ${selectedChannel.name}`} onClick={() => setDeleteTarget(selectedChannel)}><Trash2 size={14} />删除</Button>
            </div>
            <dl className="channel-facts">
              {selectedChannel.app_id && <div><dt>应用 ID</dt><dd>{selectedChannel.app_id}</dd></div>}
              {selectedChannel.api_base_url && <div className="channel-fact-wide"><dt>{selectedChannel.provider === "wechat_ilink" ? "微信服务地址" : "OneBot HTTP API"}</dt><dd><code>{selectedChannel.api_base_url}</code></dd></div>}
              {selectedChannel.self_id && <div><dt>机器人 ID</dt><dd>{selectedChannel.self_id}</dd></div>}
              <div><dt>账号凭据</dt><dd>{selectedChannel.credentials_configured ? "已存入本机凭据库" : "尚未完整配置"}</dd></div>
              <div><dt>允许联系用户</dt><dd>{selectedChannel.allowed_senders.length} 个</dd></div>
              <div><dt>回复发送方式</dt><dd>{replyModeLabel(selectedChannel.reply_mode)}</dd></div>
              <div><dt>回复预设</dt><dd>{presets.find(preset => preset.id === selectedChannel.preset_id)?.name ?? "默认聊天配置"}</dd></div>
              <div><dt>最近收到消息</dt><dd>{selectedChannel.last_received_at ? new Date(selectedChannel.last_received_at).toLocaleString() : "尚无有效消息"}</dd></div>
              {(selectedChannel.provider === "onebot11" && (selectedChannel.connection_mode === "http" || selectedChannel.connection_mode === "reverse_websocket") || selectedChannel.provider === "feishu" && selectedChannel.connection_mode === "http") && <div className="channel-fact-wide"><dt>{selectedChannel.provider === "feishu" ? "飞书 HTTP 回调路径" : selectedChannel.connection_mode === "http" ? "同机 OneBot HTTP 接收地址" : "同机 OneBot 反向 WebSocket 地址"}</dt><dd className="channel-copy-value"><code>{channelReceiveAddress(selectedChannel.provider, selectedChannel.connection_mode, selectedChannel.callback_path, API_BASE) ?? selectedChannel.callback_path}</code><Button variant="ghost" size="sm" onClick={() => void copyConnectionTarget(selectedChannel)}><Clipboard size={13} />{copyState === selectedChannel.id ? "已复制" : "复制地址"}</Button></dd></div>}
            </dl>
            <p className="channel-endpoint-note">{replyModeDescription(selectedChannel.reply_mode)}</p>
            {selectedChannel.provider === "onebot11" && (selectedChannel.connection_mode === "http" || selectedChannel.connection_mode === "reverse_websocket") && <p className="channel-endpoint-note">此地址用于同一电脑上运行的 OneBot 服务。远程服务需要可达的主机地址配置；页面不会替你生成公网地址。</p>}
            {selectedChannel.provider === "feishu" && selectedChannel.connection_mode === "http" && <p className="channel-endpoint-note">飞书 HTTP 回调还需要你配置公网可达的 HTTPS 地址。此处仅显示路径，本机地址不代表公网可访问。</p>}
            {selectedChannel.last_error && <p className="channels-error" role="alert">最近错误：{selectedChannel.last_error}</p>}
            {selectedChannel.last_event && <div className="channel-event-observation" role="note"><span>最近范围外事件 · {selectedChannel.last_event.kind} · {new Date(selectedChannel.last_event.at).toLocaleString()}</span><p>{selectedChannel.last_event.detail}</p></div>}
            {checkResults[selectedChannel.id] && <div className="channel-check-results" aria-label="连接检查结果"><h3><ShieldCheck size={15} />连接检查</h3>{checkResults[selectedChannel.id].map((check, index) => <div key={`${check.label}:${index}`} data-ok={check.ok}><span>{check.ok ? <Check size={14} /> : <span aria-hidden="true">!</span>}{check.label}</span><p>{check.detail}</p></div>)}</div>}
          </section> : <GettingStarted />}
        </div>
      </>}

      {view === "inbox" && <div className="channels-page-footer"><span>只接收已允许用户的私聊文本；{selectedChannel ? replyModeDescription(selectedChannel.reply_mode) : "回复方式在渠道配置中选择。"}</span>{orderedProviders.map(provider => <a key={provider.id} href={provider.docs_url} target="_blank" rel="noreferrer">{provider.label}资料<ExternalLink size={12} /></a>)}</div>}

      {editorOpen && <ChannelEditor channel={editorChannel} initialProvider={initialProvider} providers={orderedProviders.filter(provider => provider.supported)} presets={presets} onClose={() => { setEditorOpen(false); setEditorChannel(null); }} onSaved={afterSaved} />}
      {loginChannel && loginState && <ChannelLoginDialog channel={loginChannel} initialLogin={loginState} onClose={() => { setLoginChannel(null); setLoginState(null); }} onUpdated={refresh} />}
      <Modal open={Boolean(deleteTarget)} onClose={() => { if (!busyChannelId) setDeleteTarget(null); }} title="删除消息渠道" variant="danger" size="md" footer={(
        <>
          <Button variant="secondary" disabled={Boolean(busyChannelId)} onClick={() => setDeleteTarget(null)}>取消</Button>
          <Button variant="danger" disabled={Boolean(busyChannelId)} loading={Boolean(busyChannelId)} loadingLabel="删除中…" onClick={() => void removeChannel()}>停止并删除渠道</Button>
        </>
      )}>
        {deleteTarget && <div className="channel-delete-copy"><p>将停止“{deleteTarget.name}”的连接，并删除该渠道配置、凭据、收件箱记录和会话关联。</p><p>已有本地对话与聊天消息会保留。已发送到平台的消息不会被撤回。</p></div>}
      </Modal>
      <Modal open={pendingView !== null} onClose={() => { if (!presetMutationBusy) setPendingView(null); }} title="放弃未保存的预设修改？" size="sm" footer={(
        <>
          <Button variant="secondary" disabled={presetMutationBusy} onClick={() => setPendingView(null)}>继续编辑</Button>
          <Button variant="danger" disabled={presetMutationBusy} onClick={() => { if (pendingView) { setHasUnsavedPresetChanges(false); setView(pendingView); setPendingView(null); } }}>放弃修改并离开</Button>
        </>
      )}>
        <p className="channel-confirm-copy">当前回复预设的修改尚未保存。切换页面会放弃这些更改。</p>
      </Modal>
    </section>
  );
}

function providerOrder(provider: string) {
  return ["feishu", "onebot11", "wechat_ilink"].indexOf(provider) < 0 ? 9 : ["feishu", "onebot11", "wechat_ilink"].indexOf(provider);
}

function GettingStarted() {
  return <aside className="channel-getting-started">
    <h2>接入进度</h2>
    <ol><li><span>01</span>选择并配置一个渠道</li><li><span>02</span>验证账号与连接状态</li><li><span>03</span>指定允许联系的用户</li><li><span>04</span>选择回复预设，开始收信</li></ol>
    <p>检查结果分别显示账号、连接和最近收信状态。保存配置不表示已收到消息。</p>
  </aside>;
}

function ChannelGuide({ providers }: { providers: ChannelProvider[] }) {
  return <section className="channel-guide">
    <div className="channel-guide-heading"><span className="channel-step-label">接入指南</span><h2>连接状态按真实平台结果显示</h2><p>保存渠道后运行连接检查。凭据可读取、服务已连接、收到消息和平台确认发送分别记录。</p></div>
    <div className="channel-guide-cards">{providers.map(provider => <article key={provider.id}>
      <h3>{provider.label}</h3><p>{provider.protocol}</p>
      <ul>{provider.limitations.map(item => <li key={item}>{item}</li>)}</ul>
      <a href={provider.docs_url} target="_blank" rel="noreferrer">查看官方资料 <ExternalLink size={13} /></a>
    </article>)}</div>
    <p className="channel-guide-safety">人工审阅模式会在发送前等待用户批准；自动回复模式会在新私聊生成后直接发送，已有草稿保持原状。送达结果未知时，请在平台核对，应用不会自动重发。</p>
  </section>;
}
