import { useCallback, useEffect, useState } from "react";
import { Cloud, Database, LogIn, LogOut, RefreshCw, ShieldCheck, UserCircle } from "lucide-react";
import { useSearchParams } from "react-router-dom";
import {
  getCloudIdentity,
  getPrivacySummary,
  loginCloud,
  listSyncConflicts,
  listCloudSessions,
  logoutCloud,
  requestAccountDeletion,
  requestCloudExport,
  registerCloud,
  resolveSyncConflict,
  revokeCloudSession,
  syncCloudNow,
  updateSyncChoice,
  type CloudIdentityResponse,
  type CloudDataSummary,
  type LifecycleJob,
  type SyncConflict,
  type CloudSession,
} from "../api/cloud";
import { Badge, Button, EmptyState, Input, PageHeader, Skeleton } from "../components/ui";
import { identityPresentation, syncPresentation } from "../features/cloud/cloudPresentation";

type Section = "account" | "sync" | "privacy";

const SECTIONS: Array<{ id: Section; label: string; description: string; icon: typeof UserCircle }> = [
  { id: "account", label: "账号", description: "身份、设备与会话", icon: UserCircle },
  { id: "sync", label: "同步", description: "积压、冲突与恢复", icon: Cloud },
  { id: "privacy", label: "数据与隐私", description: "本机数据与敏感信息", icon: ShieldCheck },
];

export default function AccountCenterPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const requested = searchParams.get("section");
  const section: Section = requested === "sync" || requested === "privacy" ? requested : "account";
  const [state, setState] = useState<CloudIdentityResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [mode, setMode] = useState<"login" | "register">("login");
  const [form, setForm] = useState({ cloud_base_url: "http://127.0.0.1:8787/api/v1", email: "", password: "", display_name: "" });
  const [dataSummary, setDataSummary] = useState<CloudDataSummary | null>(null);
  const [lifecycleJob, setLifecycleJob] = useState<LifecycleJob | null>(null);
  const [conflicts, setConflicts] = useState<SyncConflict[]>([]);
  const [sessions,setSessions]=useState<CloudSession[]>([]);

  const refresh = useCallback(async () => {
    const next = await getCloudIdentity();
    setState(next);
    setLoading(false);
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    if (section !== "privacy" || state?.identity.status === "guest") return;
    void getPrivacySummary().then((result) => { if (result.ok) setDataSummary(result.data.summary); });
  }, [section, state?.identity.status]);
  useEffect(() => {
    if (section !== "sync") return;
    void listSyncConflicts().then((result) => { if (result.ok) setConflicts(result.data.conflicts); });
  }, [section, state?.sync.conflict_count]);
  useEffect(()=>{if(section!=="account"||state?.identity.status==="guest")return;void listCloudSessions().then(result=>{if(result.ok)setSessions(result.data.sessions)})},[section,state?.identity.status]);

  const authenticate = async () => {
    setBusy(true); setError("");
    const result = mode === "login"
      ? await loginCloud(form)
      : await registerCloud({ ...form, display_name: form.display_name || undefined });
    if (result.ok) {
      setState(result.data); setForm((current) => ({ ...current, password: "" }));
    } else {
      setError(result.error);
    }
    setBusy(false);
  };

  const logout = async () => {
    setBusy(true); setError("");
    const result = await logoutCloud();
    if (result.ok) setState(result.data); else setError(result.error);
    setBusy(false);
  };

  const syncNow = async () => {
    setBusy(true); setError("");
    const result = await syncCloudNow();
    if (!result.ok) setError(result.error);
    await refresh();
    setBusy(false);
  };

  const chooseSync = async (choice: "sync" | "local_only") => {
    setBusy(true); setError("");
    const result = await updateSyncChoice(choice);
    if (!result.ok) setError(result.error);
    await refresh(); setBusy(false);
  };

  const resolveConflict = async (id:string,resolution:"local"|"remote") => {
    setBusy(true);setError("");const result=await resolveSyncConflict(id,resolution);if(result.ok){setConflicts(result.data.conflicts);await refresh();}else setError(result.error);setBusy(false);
  };
  const revokeSession=async(id:string)=>{setBusy(true);setError("");const result=await revokeCloudSession(id);if(result.ok)setSessions(result.data.sessions);else setError(result.error);setBusy(false)};

  const requestExport = async () => {
    setBusy(true); setError("");
    const result = await requestCloudExport();
    if (result.ok) setLifecycleJob(result.data.job); else setError(result.error);
    setBusy(false);
  };

  const deleteAccount = async () => {
    if (window.prompt("此操作将永久删除云端账号。请输入 DELETE 继续：") !== "DELETE") return;
    setBusy(true); setError("");
    const result = await requestAccountDeletion();
    if (result.ok) { setLifecycleJob(result.data.job); await refresh(); }
    else setError(result.error);
    setBusy(false);
  };

  const identity = state?.identity;
  const identityMeta = identity ? identityPresentation(identity) : null;
  const syncMeta = state ? syncPresentation(state.sync) : null;

  return (
    <div className="cloud-page page-canvas">
      <PageHeader
        title="账号中心"
        description="本地功能始终可用；登录后再由你决定是否启用跨设备同步。"
        actions={identityMeta ? <Badge tone={identityMeta.tone}>{identityMeta.label}</Badge> : undefined}
      />
      <div className="cloud-layout">
        <nav className="cloud-section-nav" aria-label="账号中心分类">
          {SECTIONS.map((item) => {
            const Icon = item.icon;
            return <button key={item.id} type="button" aria-current={section === item.id ? "page" : undefined} onClick={() => setSearchParams(item.id === "account" ? {} : { section: item.id })}>
              <Icon aria-hidden="true" />
              <span><strong>{item.label}</strong><small>{item.description}</small></span>
            </button>;
          })}
          <div className="cloud-local-note"><Database aria-hidden="true" /><span><strong>Local-first</strong>云端不可用时，本地 Agent、任务、画布与资料仍可使用。</span></div>
        </nav>
        <main className="cloud-content scrollbar-thin">
          {loading ? <><Skeleton className="h-28" /><Skeleton className="mt-4 h-56" /></> : null}
          {!loading && !state ? <EmptyState title="账号状态暂时不可用" description="本地核心功能不受影响，请稍后重试。" action={<Button variant="secondary" onClick={() => void refresh()}>重新载入</Button>} /> : null}
          {state && section === "account" ? <section className="cloud-section-stack" aria-labelledby="account-section-title">
            <div className="cloud-section-heading"><div><span>IDENTITY</span><h2 id="account-section-title">账号与设备</h2><p>{identityMeta?.description}</p></div><Button size="sm" variant="secondary" onClick={() => void refresh()}><RefreshCw />刷新</Button></div>
            {identity?.status === "guest" ? <div className="cloud-card cloud-auth-card">
              <div className="cloud-mode-switch" role="tablist" aria-label="账号操作"><button type="button" role="tab" aria-selected={mode === "login"} onClick={() => setMode("login")}>登录</button><button type="button" role="tab" aria-selected={mode === "register"} onClick={() => setMode("register")}>注册</button></div>
              <div className="cloud-form-grid">
                {mode === "register" ? <Input label="显示名称" value={form.display_name} onChange={(event) => setForm({ ...form, display_name: event.target.value })} autoComplete="name" /> : null}
                <Input label="邮箱" type="email" value={form.email} onChange={(event) => setForm({ ...form, email: event.target.value })} autoComplete="email" />
                <Input label="密码" type="password" value={form.password} onChange={(event) => setForm({ ...form, password: event.target.value })} autoComplete={mode === "login" ? "current-password" : "new-password"} />
              </div>
              <details className="cloud-disclosure"><summary>云服务地址</summary><Input label="Cloud API" value={form.cloud_base_url} onChange={(event) => setForm({ ...form, cloud_base_url: event.target.value })} hint="私有部署默认使用你的 Cloud API 地址。" /></details>
              <div className="cloud-card-actions"><p>登录不会自动上传现有数据，也不会上传本机 Provider 密钥。</p><Button disabled={busy || !form.email.trim() || !form.password} onClick={() => void authenticate()}><LogIn />{busy ? "正在连接…" : mode === "login" ? "登录" : "创建账号"}</Button></div>
            </div> : <><div className="cloud-card">
              <div className="cloud-account-summary"><span className="cloud-account-avatar"><UserCircle /></span><div><h3>{identity?.email || "已登录账号"}</h3><p>本机身份 {identity?.local_identity_id}</p></div><Badge tone="success">会话有效</Badge></div>
              <dl className="cloud-data-list"><div><dt>当前设备</dt><dd>{identity?.device_id}</dd></div><div><dt>云端账号</dt><dd>{identity?.account_id}</dd></div><div><dt>密钥策略</dt><dd>登录凭据保存在系统凭据库；Provider 密钥保持本机专用</dd></div></dl>
              <div className="cloud-card-actions"><p>退出登录只清除本机云会话，不删除云端账号和本地工作数据。</p><Button variant="secondary" disabled={busy} onClick={() => void logout()}><LogOut />退出登录</Button></div>
            </div>{sessions.length?<div className="cloud-card"><h3>设备与会话</h3><div className="cloud-session-list">{sessions.map(session=><article key={session.id}><div><strong>{session.device_label}</strong><span>最近活动：{new Date(session.last_seen_at).toLocaleString()}</span></div>{session.current?<Badge tone="info">当前设备</Badge>:session.revoked_at?<Badge>已退出</Badge>:<Button size="sm" variant="secondary" disabled={busy} onClick={()=>void revokeSession(session.id)}>退出该设备</Button>}</article>)}</div></div>:null}{state.sync_existing_choice === "undecided" ? <div className="cloud-card"><div className="cloud-card-title"><div><h3>如何处理现有本地数据？</h3><p>登录不会自动上传。请明确选择，之后仍可在同步中心修改。</p></div><Cloud /></div><div className="setup-choice-row"><button type="button" disabled={busy} onClick={() => void chooseSync("sync")}><strong>同步已有数据</strong><span>把选择的数据类型加入增量同步</span></button><button type="button" disabled={busy} onClick={() => void chooseSync("local_only")}><strong>保持仅本地</strong><span>账号保持登录，但不上传工作数据</span></button></div></div> : null}</>}
          </section> : null}
          {state && section === "sync" ? <section className="cloud-section-stack" aria-labelledby="sync-section-title">
            <div className="cloud-section-heading"><div><span>SYNC</span><h2 id="sync-section-title">同步中心</h2><p>本地变更先进入 Outbox，联网后按 revision 与 cursor 增量交换。</p></div><Button disabled={busy || identity?.status === "guest" || state.sync_existing_choice !== "sync"} onClick={() => void syncNow()}><RefreshCw />{busy ? "同步中…" : "立即同步"}</Button></div>
            <div className="cloud-metric-grid"><article><span>待同步</span><strong>{state.sync.pending_count}</strong></article><article><span>冲突</span><strong>{state.sync.conflict_count}</strong></article><article><span>失败</span><strong>{state.sync.error_count}</strong></article><article><span>游标</span><strong>{state.sync.cursor || "—"}</strong></article></div>
            <div className="cloud-card"><div className="cloud-card-title"><div><h3>{syncMeta?.label}</h3><p>{syncMeta?.detail}</p></div><Badge tone={syncMeta?.tone || "default"}>{identity?.status === "guest" ? "登录后可用" : "增量同步"}</Badge></div>{state.sync.last_error_code ? <p className="cloud-inline-error" role="status">最近错误：{state.sync.last_error_code}。本地变更仍保留，可稍后重试。</p> : null}<p className="cloud-muted">删除使用 tombstone；旧设备的陈旧数据不能复活已经删除的云端实体。</p></div>
            {identity?.status !== "guest" ? <div className="cloud-card"><h3>同步策略</h3><p>{state.sync_existing_choice === "sync" ? "已允许同步选择的数据类型。" : state.sync_existing_choice === "local_only" ? "当前保持仅本地，不会上传工作数据。" : "尚未选择。"}</p><div className="cloud-card-actions"><Button variant={state.sync_existing_choice === "sync" ? "primary" : "secondary"} disabled={busy} onClick={() => void chooseSync("sync")}>启用同步</Button><Button variant={state.sync_existing_choice === "local_only" ? "primary" : "secondary"} disabled={busy} onClick={() => void chooseSync("local_only")}>保持仅本地</Button></div></div> : null}
            {conflicts.length ? <div className="cloud-card"><h3>待处理冲突</h3><p>不会静默覆盖。选择保留本机版本会基于云端 revision 创建新变更。</p><div className="cloud-conflict-list">{conflicts.map(conflict=><article key={conflict.id}><div><strong>{conflict.entity_type}</strong><span>{conflict.entity_id}</span><small>本机 r{conflict.local_revision} · 云端 r{conflict.remote_revision}</small></div><div><Button size="sm" disabled={busy} onClick={()=>void resolveConflict(conflict.id,"local")}>保留本机</Button><Button size="sm" variant="secondary" disabled={busy||(conflict.remote_operation==="upsert"&&!conflict.remote_payload)} onClick={()=>void resolveConflict(conflict.id,"remote")}>采用云端</Button></div></article>)}</div></div>:null}
          </section> : null}
          {state && section === "privacy" ? <section className="cloud-section-stack" aria-labelledby="privacy-section-title">
            <div className="cloud-section-heading"><div><span>PRIVACY</span><h2 id="privacy-section-title">数据与隐私</h2><p>同步内容、退出登录和永久注销使用不同语义。</p></div></div>
            <div className="cloud-card"><div className="cloud-card-title"><div><h3>默认不上传的内容</h3><p>敏感信息继续由本机 SecretStore 管理。</p></div><ShieldCheck /></div><ul className="cloud-bullet-list"><li>LLM、STT、TTS、AnySearch 与 MCP 密钥</li><li>完整 MCP 参数和完整工具结果</li><li>未经选择的对话、记忆、文件正文与二进制</li></ul></div>
            <div className="cloud-card"><h3>本机与云端生命周期</h3><dl className="cloud-data-list"><div><dt>退出登录</dt><dd>结束当前会话，不删除云端数据</dd></div><div><dt>注销账号</dt><dd>撤销全部会话并启动永久 DeletionJob</dd></div><div><dt>本机清理</dt><dd>始终由你单独选择，不与云端删除混为一体</dd></div></dl></div>
            {identity?.status !== "guest" ? <div className="cloud-card"><div className="cloud-card-title"><div><h3>云端数据摘要</h3><p>这里只展示实体数量与对象占用，不读取正文。</p></div><Database /></div><dl className="cloud-data-list">{Object.entries(dataSummary?.counts || {}).map(([key,value]) => <div key={key}><dt>{key}</dt><dd>{value} 项</dd></div>)}<div><dt>对象存储</dt><dd>{dataSummary ? `${dataSummary.object_bytes} bytes` : "正在读取…"}</dd></div></dl><div className="cloud-card-actions"><Button variant="secondary" disabled={busy} onClick={() => void requestExport()}>导出我的云端数据</Button><Button variant="danger" disabled={busy || identity?.status === "closing"} onClick={() => void deleteAccount()}>永久注销云端账号</Button></div>{lifecycleJob ? <p className="cloud-inline-error" role="status">{lifecycleJob.kind === "export" ? "导出" : "注销"}任务：{lifecycleJob.status} · {lifecycleJob.progress}%{lifecycleJob.error_code ? ` · ${lifecycleJob.error_code}` : ""}</p> : null}</div> : null}
          </section> : null}
          {error ? <p className="cloud-error" role="alert">{error}</p> : null}
        </main>
      </div>
    </div>
  );
}
