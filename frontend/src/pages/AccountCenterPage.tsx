import { useCallback, useEffect, useState } from "react";
import { Cloud, Database, LogIn, LogOut, RefreshCw, ShieldCheck, UserCircle } from "lucide-react";
import { useSearchParams } from "react-router-dom";
import {
  getCloudIdentity,
  loginCloud,
  logoutCloud,
  registerCloud,
  syncCloudNow,
  type CloudIdentityResponse,
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

  const refresh = useCallback(async () => {
    const next = await getCloudIdentity();
    setState(next);
    setLoading(false);
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);

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
            </div> : <div className="cloud-card">
              <div className="cloud-account-summary"><span className="cloud-account-avatar"><UserCircle /></span><div><h3>{identity?.email || "已登录账号"}</h3><p>本机身份 {identity?.local_identity_id}</p></div><Badge tone="success">会话有效</Badge></div>
              <dl className="cloud-data-list"><div><dt>当前设备</dt><dd>{identity?.device_id}</dd></div><div><dt>云端账号</dt><dd>{identity?.account_id}</dd></div><div><dt>密钥策略</dt><dd>登录凭据保存在系统凭据库；Provider 密钥保持本机专用</dd></div></dl>
              <div className="cloud-card-actions"><p>退出登录只清除本机云会话，不删除云端账号和本地工作数据。</p><Button variant="secondary" disabled={busy} onClick={() => void logout()}><LogOut />退出登录</Button></div>
            </div>}
          </section> : null}
          {state && section === "sync" ? <section className="cloud-section-stack" aria-labelledby="sync-section-title">
            <div className="cloud-section-heading"><div><span>SYNC</span><h2 id="sync-section-title">同步中心</h2><p>本地变更先进入 Outbox，联网后按 revision 与 cursor 增量交换。</p></div><Button disabled={busy || identity?.status === "guest"} onClick={() => void syncNow()}><RefreshCw />{busy ? "同步中…" : "立即同步"}</Button></div>
            <div className="cloud-metric-grid"><article><span>待同步</span><strong>{state.sync.pending_count}</strong></article><article><span>冲突</span><strong>{state.sync.conflict_count}</strong></article><article><span>失败</span><strong>{state.sync.error_count}</strong></article><article><span>游标</span><strong>{state.sync.cursor || "—"}</strong></article></div>
            <div className="cloud-card"><div className="cloud-card-title"><div><h3>{syncMeta?.label}</h3><p>{syncMeta?.detail}</p></div><Badge tone={syncMeta?.tone || "default"}>{identity?.status === "guest" ? "登录后可用" : "增量同步"}</Badge></div>{state.sync.last_error_code ? <p className="cloud-inline-error" role="status">最近错误：{state.sync.last_error_code}。本地变更仍保留，可稍后重试。</p> : null}<p className="cloud-muted">删除使用 tombstone；旧设备的陈旧数据不能复活已经删除的云端实体。</p></div>
          </section> : null}
          {state && section === "privacy" ? <section className="cloud-section-stack" aria-labelledby="privacy-section-title">
            <div className="cloud-section-heading"><div><span>PRIVACY</span><h2 id="privacy-section-title">数据与隐私</h2><p>同步内容、退出登录和永久注销使用不同语义。</p></div></div>
            <div className="cloud-card"><div className="cloud-card-title"><div><h3>默认不上传的内容</h3><p>敏感信息继续由本机 SecretStore 管理。</p></div><ShieldCheck /></div><ul className="cloud-bullet-list"><li>LLM、STT、TTS、AnySearch 与 MCP 密钥</li><li>完整 MCP 参数和完整工具结果</li><li>未经选择的对话、记忆、文件正文与二进制</li></ul></div>
            <div className="cloud-card"><h3>本机与云端生命周期</h3><dl className="cloud-data-list"><div><dt>退出登录</dt><dd>结束当前会话，不删除云端数据</dd></div><div><dt>注销账号</dt><dd>撤销全部会话并启动永久 DeletionJob</dd></div><div><dt>本机清理</dt><dd>始终由你单独选择，不与云端删除混为一体</dd></div></dl></div>
          </section> : null}
          {error ? <p className="cloud-error" role="alert">{error}</p> : null}
        </main>
      </div>
    </div>
  );
}
