import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button, ErrorState, Input, Modal, Skeleton } from "../../components/ui";
import { bindPluginServiceChannel, ensurePluginService, getPluginService, PluginServiceError, revokePluginServiceCredential, savePluginServiceCredential, startPluginService, stopPluginService, type PluginServiceStatus, type ServiceLaunch } from "../../api/pluginServices";
import { nativeBrowserAvailable } from "../../api/browser";
import { openInSystemBrowser } from "../../components/chat/externalLink";
import PluginSettingsDialog from "../settings/canvas/PluginSettingsDialog";
import "./pluginService.css";

const stateText = { stopped: "尚未启动", running: "运行中", exited: "进程已退出" };
const channelText: Record<string, string> = { configured: "已配置，等待连接", connected: "已连接", disabled: "已停用", error: "连接异常" };
export default function PluginServicePanel({ pluginId, serviceId }: { pluginId: string; serviceId: string }) {
  const navigate = useNavigate();
  const [data, setData] = useState<PluginServiceStatus | null>(null);
  const [error, setError] = useState("");
  const [loadError, setLoadError] = useState("");
  const [busy, setBusy] = useState(false);
  const [dialog, setDialog] = useState<"settings" | "start" | "stop" | "credential" | null>(null);
  const [approvedLaunch, setApprovedLaunch] = useState<ServiceLaunch | null>(null);
  const [approvedRevision, setApprovedRevision] = useState<string | undefined>();
  const [allowModel, setAllowModel] = useState(false);
  const [openAfterStart, setOpenAfterStart] = useState(false);
  const [credentialTarget, setCredentialTarget] = useState<{ pluginId: string; serviceId: string; revision: string; origin: string; configured: boolean } | null>(null);
  const [credentialToken, setCredentialToken] = useState("");
  const [confirmRevoke, setConfirmRevoke] = useState(false);
  const alive = useRef(true), flight = useRef(false), epoch = useRef(0);
  const refresh = useCallback(async () => {
    if (flight.current) return;
    flight.current = true; const current = epoch.current;
    try { const next = await getPluginService(pluginId, serviceId); if (alive.current && epoch.current === current) { setData(next); setLoadError(""); } }
    catch (cause) { if (alive.current && epoch.current === current) setLoadError(String(cause)); }
    finally { flight.current = false; }
  }, [pluginId, serviceId]);
  useEffect(() => {
    alive.current = true; ++epoch.current; setData(null); void refresh();
    setDialog(previous => previous === "credential" ? null : previous);
    setCredentialToken(""); setCredentialTarget(null);
    const timer = window.setInterval(() => { if (!document.hidden && !flight.current) void refresh(); }, 4000);
    return () => { alive.current = false; ++epoch.current; window.clearInterval(timer); };
  }, [refresh]);
  async function action(operation: () => Promise<unknown>) {
    setBusy(true); setError("");
    try { await operation(); if (alive.current) { setDialog(null); await refresh(); } }
    catch (cause) { if (alive.current) setError(String(cause)); }
    finally { if (alive.current) setBusy(false); }
  }
  async function openWeb() {
    if (!data?.web_url) return;
    // Externally launched local pages keep their existing unmanaged path.
    if (data.deployment === "local" && data.state !== "running" && data.web_available) {
      if (nativeBrowserAvailable()) navigate(`/browser?url=${encodeURIComponent(data.web_url)}`);
      else await action(() => openInSystemBrowser(data.web_url!));
      return;
    }
    setBusy(true); setError("");
    try {
      const next = await ensurePluginService(pluginId, serviceId, { acquire_page: nativeBrowserAvailable() });
      await present(next);
    } catch (cause) {
      if (cause instanceof PluginServiceError && cause.details?.code === "launch_confirmation_required") {
        setApprovedLaunch(cause.details.launch as ServiceLaunch);
        setApprovedRevision(cause.details.configuration_revision as string);
        setAllowModel(false); setOpenAfterStart(true); setDialog("start");
      } else setError(cause instanceof Error ? cause.message : String(cause));
    } finally { if (alive.current) setBusy(false); }
  }
  async function present(next: PluginServiceStatus) {
    if (alive.current) setData(next);
    if (nativeBrowserAvailable()) {
      if (!next.browser_tab_id) throw new Error("服务已就绪，但没有返回管理页会话，请重新打开。");
      navigate(`/browser?tab=${encodeURIComponent(next.browser_tab_id)}`);
    } else if (next.web_url) await openInSystemBrowser(next.web_url);
  }
  function prepareStart() {
    setApprovedLaunch(data?.launch ?? null);
    setApprovedRevision(data?.configuration_revision ?? undefined);
    setAllowModel(false); setOpenAfterStart(false); setError(""); setDialog("start");
  }
  async function confirmStart() {
    const options = { configuration_revision: approvedRevision, ...(allowModel ? { allow_model: true } : {}) };
    if (openAfterStart) {
      const next = await ensurePluginService(pluginId, serviceId, { ...options, launch: approvedLaunch!, acquire_page: nativeBrowserAvailable() });
      await present(next);
    } else await startPluginService(pluginId, serviceId, approvedLaunch!, options);
  }
  function openCredential() {
    if (!data?.configuration_revision || !data.web_url) return;
    setCredentialTarget({ pluginId, serviceId, revision: data.configuration_revision, origin: new URL(data.web_url).origin, configured: data.cloud_auth_configured });
    setCredentialToken(""); setConfirmRevoke(false); setError(""); setDialog("credential");
  }
  async function saveCredential() {
    if (!credentialTarget) return;
    const { pluginId: targetPlugin, serviceId: targetService, revision } = credentialTarget;
    if (confirmRevoke) await revokePluginServiceCredential(targetPlugin, targetService, revision);
    else await savePluginServiceCredential(targetPlugin, targetService, revision, credentialToken);
    setCredentialToken(""); setCredentialTarget(null);
  }
  if (!data) return loadError ? <ErrorState title="服务状态无法加载" description={loadError} action={<Button onClick={() => void refresh()}>重试</Button>} /> : <Skeleton className="h-40" />;
  return <section className="plugin-service-panel" aria-label={`${data.title} 服务管理`}>
    <div className="plugin-service-toolbar">
      {data.state === "running" ? <Button variant="secondary" disabled={busy} onClick={() => setDialog("stop")}>{data.deployment === "cloud" ? "断开服务" : "停止服务"}</Button>
        : data.launch && !data.web_available && <Button disabled={busy} onClick={prepareStart}>启动服务</Button>}
      {data.web_url && <Button variant="secondary" disabled={busy} onClick={() => void openWeb()}>{nativeBrowserAvailable() ? "打开管理页" : "在浏览器打开管理页"}</Button>}
      <Button variant="secondary" disabled={busy} onClick={() => setDialog("settings")}>运行配置</Button>
      {data.cloud_auth === "bearer" && <Button variant="secondary" disabled={busy || !data.configuration_revision} onClick={openCredential}>服务认证</Button>}
      <Button variant="ghost" disabled={busy} onClick={() => void refresh()}>刷新状态</Button>
    </div>
    {error && dialog !== "start" && dialog !== "stop" && dialog !== "credential" && <ErrorState title="操作未完成" description={error} />}
    {loadError && <ErrorState title="状态刷新失败，以下保留上次观察" description={loadError} action={<Button onClick={() => void refresh()}>重试</Button>} />}
    {data.configuration_error && <p role="status" className="plugin-service-notice">{data.configuration_error}。请打开运行配置。</p>}
    <dl className="plugin-service-status">
      <div><dt>{data.deployment === "cloud" ? "云端连接" : "托管进程"}</dt><dd>{data.deployment === "cloud" ? (data.instance_id ? "已连接" : "尚未连接") : stateText[data.state]}{data.exit_code !== null ? ` · 退出码 ${data.exit_code}` : ""}</dd></div>
      <div><dt>Web 管理页</dt><dd>{data.web_available ? "可访问" : "暂不可访问"}{data.web_error && <span>{data.web_error}</span>}</dd></div>
      <div><dt>服务检查</dt><dd>{data.service_health === "ready" ? (data.health_check === "declared" ? "健康检查通过" : "管理页可访问") : data.health_error || "尚未就绪"}</dd></div>
      {data.mcp_server_id && <div><dt>MCP 能力</dt><dd>{data.mcp_status === "ready" ? "已连接，可调用" : "尚未就绪"}</dd></div>}
      {data.cloud_auth === "bearer" && <div><dt>服务认证</dt><dd>{data.cloud_auth_configured ? "已配置令牌" : "待配置令牌"}</dd></div>}
      <div><dt>当前使用</dt><dd>{data.task_uses} 个任务 · {data.page_uses} 个内置管理页</dd></div>
      <div><dt>模型来源</dt><dd>{data.model_mode === "platform_chat" ? (data.model_allowed ? "使用忆涟当前任务的对话模型" : "尚未允许使用平台模型") : data.model_mode === "independent" ? "由服务独立配置" : "未使用平台模型"}</dd></div>
      {data.channel_id_key && <div><dt>OneBot 渠道</dt><dd>{data.channel ? `${data.channel.name} · ${channelText[data.channel.status] ?? data.channel.status}` : data.channel_id ? "关联渠道已不存在" : "尚未关联渠道"}</dd></div>}
    </dl>
    {data.deployment === "local" && data.web_available && data.state !== "running" && <p className="plugin-service-notice">管理页已有服务响应。此服务在插件之外启动，可打开管理页；如需由插件管理，请先结束外部启动的服务。</p>}
    {data.model_mode === "platform_chat" && !data.model_allowed && data.state === "running" && <p className="plugin-service-notice">需要平台模型时，可在重新启动服务时确认使用。</p>}
    {data.channel_id_key && <div className="plugin-service-channel">
      <label htmlFor={`plugin-channel-${serviceId}`}>关联已有 QQ 渠道</label>
      <select id={`plugin-channel-${serviceId}`} disabled={busy} value={data.channel_id ?? ""} onChange={event => {
        const id = event.target.value;
        void action(() => bindPluginServiceChannel(pluginId, data.settings_id, data.channel_id_key!, id));
      }}>
        <option value="">选择 OneBot 渠道</option>
        {data.channels.map(channel => <option key={channel.id} value={channel.id}>{channel.name}{channel.self_id ? ` · ${channel.self_id}` : ""}</option>)}
      </select>
      <Button variant="secondary" onClick={() => navigate(`/software/channels${data.channel ? `?channel=${encodeURIComponent(data.channel.id)}` : "?platform=qq"}`)}>{data.channel ? "进入消息渠道" : "配置消息渠道"}</Button>
    </div>}
    {data.channel_id_key && <p className="plugin-service-help">在管理页完成 QQ 登录，并将 OneBot 反向 WebSocket 配置到关联渠道。进程运行和管理页可访问不代表账号已登录或消息已送达。</p>}
    {(data.pid !== null || data.web_url || data.channel?.self_id) && <details className="plugin-service-details">
      <summary>运行详情</summary>
      <dl className="plugin-service-status">
        {data.pid !== null && <div><dt>进程 ID</dt><dd>{data.pid}</dd></div>}
        {data.web_url && <div><dt>管理页地址</dt><dd>{data.web_url}</dd></div>}
        {data.channel?.self_id && <div><dt>QQ 账号</dt><dd>{data.channel.self_id}</dd></div>}
      </dl>
    </details>}
    {dialog === "settings" && <PluginSettingsDialog nodeId={`plugin:${pluginId}:${data.settings_id}`} onClose={() => { setDialog(null); void refresh(); }} />}
    {dialog === "credential" && credentialTarget && <Modal open title={confirmRevoke ? "撤销服务令牌" : "云端服务认证"} onClose={() => {
      if (!busy) { setDialog(null); setCredentialToken(""); setCredentialTarget(null); }
    }} footer={<>
      {confirmRevoke ? <Button variant="secondary" disabled={busy} onClick={() => { setConfirmRevoke(false); setError(""); }}>返回</Button>
        : credentialTarget.configured && <Button variant="secondary" disabled={busy} onClick={() => { setConfirmRevoke(true); setError(""); }}>撤销令牌</Button>}
      <Button variant={confirmRevoke ? "danger" : "primary"} disabled={busy || (!confirmRevoke && !credentialToken.trim())} onClick={() => void action(saveCredential)}>{busy ? "处理中…" : confirmRevoke ? "确认撤销" : "保存令牌"}</Button>
    </>}>
      <p className="mb-3 break-all">服务来源 {credentialTarget.origin}</p>
      {confirmRevoke ? <p>撤销后，忆涟需要新的令牌才能通过此服务的 MCP 认证和健康检查。</p>
        : <Input type="password" autoComplete="new-password" label={credentialTarget.configured ? "新 Bearer 令牌" : "Bearer 令牌"} value={credentialToken} onChange={event => setCredentialToken(event.target.value)} disabled={busy} />}
      {error && <p className="mt-2" role="alert">{error}</p>}
    </Modal>}
    {(dialog === "start" || dialog === "stop") && <Modal open title={`${dialog === "start" ? "启动" : data.deployment === "cloud" ? "断开" : "停止"}${data.title}`} onClose={() => { if (!busy) setDialog(null); }} footer={<Button disabled={busy} onClick={() => void action(() => dialog === "start" ? confirmStart() : stopPluginService(pluginId, serviceId))}>{busy ? "处理中…" : dialog === "start" ? "确认启动" : data.deployment === "cloud" ? "确认断开" : "确认停止"}</Button>}>
      {dialog === "start" ? <><p>将使用以下程序启动{data.title}，启动后可打开管理页。</p><dl className="plugin-service-command"><dt>程序</dt><dd>{approvedLaunch?.executable}</dd><dt>参数</dt><dd>{JSON.stringify(approvedLaunch?.args)}</dd><dt>工作目录</dt><dd>{approvedLaunch?.working_directory}</dd></dl>
        {data.model_mode === "platform_chat" && <label className="flex items-start gap-2"><input type="checkbox" checked={allowModel} onChange={event => setAllowModel(event.target.checked)} disabled={busy} /><span>允许此服务在当前任务中使用忆涟的对话模型</span></label>}
      </> : data.deployment === "cloud" ? <p>将断开忆涟与此服务的连接，云端进程由部署端管理。</p> : <p>将停止本插件启动的{data.title}进程及其子进程，并断开相应连接。</p>}
      {dialog === "stop" && data.task_uses > 0 && <p className="plugin-service-notice">有 {data.task_uses} 个任务正在使用此服务，请先停止或等待这些任务结束。</p>}
      {error && <p role="alert">{error}</p>}
    </Modal>}
  </section>;
}
