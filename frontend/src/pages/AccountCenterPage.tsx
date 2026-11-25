import { useCallback, useEffect, useState } from "react";
import { Cloud, LogIn, LogOut, RefreshCw, ShieldCheck, UserCircle } from "lucide-react";
import { useSearchParams } from "react-router-dom";
import {
  getCloudIdentity,
  getLifecycleJob,
  loginCloud,
  listSyncConflicts,
  listCloudSessions,
  logoutCloud,
  requestAccountDeletion,
  requestCloudExport,
  registerCloud,
  resolveConflictChoice,
  revokeCloudSession,
  syncCloudNow,
  updateSyncChoice,
  type CloudIdentityResponse,
  type CloudConflictRecord,
  type LifecycleJob,
  type CloudSession,
  type LogoutReport,
} from "../api/cloud";
import { Badge, Button, EmptyState, Input, Skeleton, Tabs, ConfirmDialog } from "../components/ui";
import PageShell from "../components/layout/PageShell";
import { identityPresentation, syncPresentation } from "../features/cloud/cloudPresentation";
import { LifecycleJobsPanel } from "../features/cloud/LifecycleJobsPanel";
import { AccountProfilePanel } from "../features/cloud/AccountProfilePanel";
import { ConflictResolutionPanel } from "../features/cloud/ConflictResolutionPanel";
import { SyncPrivacyPanel } from "../features/cloud/SyncPrivacyPanel";
import { type ConflictItem, type ResolutionChoice } from "../features/cloud/conflictModel";
import {
  clearConflictChoiceRequest,
  getConflictChoiceRequest,
  retainConflictRequestsForScope,
} from "../features/cloud/conflictRequest";

type Section = "account" | "sync" | "privacy";

const SECTIONS: Array<{ id: Section; label: string; icon: typeof UserCircle }> = [
  { id: "account", label: "账号", icon: UserCircle },
  { id: "sync", label: "同步", icon: Cloud },
  { id: "privacy", label: "数据与隐私", icon: ShieldCheck },
];

export default function AccountCenterPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const requested = searchParams.get("section");
  const section: Section = requested === "sync" || requested === "privacy" ? requested : "account";
  const [state, setState] = useState<CloudIdentityResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [error, setError] = useState("");
  const [mode, setMode] = useState<"login" | "register">("login");
  const [showSwitchLogin, setShowSwitchLogin] = useState(false);
  const [form, setForm] = useState({ cloud_base_url: "http://127.0.0.1:8787/api/v1", email: "", password: "", display_name: "" });
  const [lifecycleJob, setLifecycleJob] = useState<LifecycleJob | null>(null);
  const [conflictRecords, setConflictRecords] = useState<CloudConflictRecord[]>([]);
  const [conflictsLoading, setConflictsLoading] = useState(true);
  const [conflictsLoadError, setConflictsLoadError] = useState("");
  const [sessions, setSessions] = useState<CloudSession[]>([]);
  const [lastLogoutReport, setLastLogoutReport] = useState<LogoutReport | null>(null);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);

  const refresh = useCallback(async () => {
    const next = await getCloudIdentity();
    setState(next);
    setLoading(false);
  }, []);

  const refreshConflictRecords = useCallback(async (): Promise<boolean> => {
    setConflictsLoading(true);
    const result = await listSyncConflicts();
    if (result.ok) {
      setConflictRecords(result.data.records);
      setConflictsLoadError("");
      setConflictsLoading(false);
      return true;
    }
    setConflictsLoadError(result.error);
    setConflictsLoading(false);
    return false;
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    if (section !== "sync") return;
    void refreshConflictRecords();
  }, [section, refreshConflictRecords]);
  useEffect(() => {
    if (loading) return;
    const identity = state?.identity;
    retainConflictRequestsForScope(identity?.local_identity_id && identity.account_id
      ? { local_identity_id: identity.local_identity_id, account_id: identity.account_id }
      : null);
  }, [loading, state?.identity.local_identity_id, state?.identity.account_id]);
  useEffect(() => {
    if (section !== "account" || !state || state.identity.status === "guest") return;
    void listCloudSessions().then((result) => {
      if (result.ok) setSessions(result.data.sessions);
    });
  }, [section, state?.identity.status]);

  const authenticate = async () => {
    setBusy(true); setError("");
    const result = mode === "login"
      ? await loginCloud(form)
      : await registerCloud({ ...form, display_name: form.display_name || undefined });
    if (result.ok) {
      setForm((current) => ({ ...current, password: "" }));
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("restart_application");
      } catch {
        setError("账号数据已隔离保存。请重新启动应用以进入该账号。当前窗口不会加载新账号数据。");
      }
    } else {
      setError(result.error);
    }
    setBusy(false);
  };

  const logout = async () => {
    setBusy(true); setError("");
    const result = await logoutCloud();
    if (result.ok) {
      retainConflictRequestsForScope(null);
      setLastLogoutReport(result.data.report);
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("restart_application");
      } catch {
        setError("已安全退出云账号。请重新启动应用以返回游客 Profile。");
      }
    } else setError(result.error);
    setBusy(false);
  };

  const syncNow = async () => {
    setBusy(true); setError("");
    setSyncing(true);
    const result = await syncCloudNow();
    if (!result.ok) setError(result.error);
    await refresh();
    await refreshConflictRecords();
    setSyncing(false);
    setBusy(false);
  };

  const chooseSync = async (choice: "sync" | "local_only") => {
    setBusy(true); setError("");
    const result = await updateSyncChoice(choice);
    if (!result.ok) setError(result.error);
    await refresh(); setBusy(false);
  };

  const revokeSession = async (id: string) => {
    setBusy(true);
    setError("");
    const result = await revokeCloudSession(id);
    if (result.ok) setSessions(result.data.sessions);
    else setError(result.error);
    setBusy(false);
  };

  const conflictItems: ConflictItem[] = conflictRecords;

  const handleResolveConflictItem = async (
    conflict: ConflictItem,
    choice: ResolutionChoice
  ): Promise<{ ok: boolean; message?: string }> => {
    const identity = state?.identity;
    if (!identity?.local_identity_id || !identity.account_id) {
      return { ok: false, message: "当前账号作用域不可用，请刷新后重试" };
    }
    const scope = {
      local_identity_id: identity.local_identity_id,
      account_id: identity.account_id,
    };
    const payload = getConflictChoiceRequest(scope, conflict, choice);
    setBusy(true);
    setError("");
    const result = await resolveConflictChoice(conflict.id, payload);
    setBusy(false);
    if (result.ok) {
      clearConflictChoiceRequest(scope, conflict, choice);
      setConflictRecords(result.data.records);
      setConflictsLoadError("");
      await refresh();
      return { ok: true };
    }
    if (result.status === 409) {
      clearConflictChoiceRequest(scope, conflict, choice);
      const refreshed = await refreshConflictRecords();
      await refresh();
      const message = refreshed
        ? "冲突内容或账号作用域已变化，请核对最新记录后重新选择"
        : "操作被拒绝，冲突记录刷新失败；已保留上次读取的内容，请重试刷新";
      setError(message);
      return { ok: false, message };
    }
    setError(result.error);
    return { ok: false, message: result.error };
  };

  const requestExport = async () => {
    setBusy(true); setError("");
    const result = await requestCloudExport();
    if (result.ok) setLifecycleJob(result.data.job); else setError(result.error);
    setBusy(false);
  };

  const refreshJob = useCallback(async (kind: LifecycleJob["kind"], id: string) => {
    setBusy(true);
    setError("");
    const result = await getLifecycleJob(kind, id);
    if (result.ok) setLifecycleJob(result.data.job);
    else setError(result.error);
    setBusy(false);
  }, []);

  const confirmDeleteAccount = async () => {
    setDeleteDialogOpen(false);
    setBusy(true);
    setError("");
    const result = await requestAccountDeletion();
    if (result.ok) {
      setLifecycleJob(result.data.job);
      await refresh();
    } else {
      setError(result.error);
    }
    setBusy(false);
  };

  const identity = state?.identity;
  const identityMeta = identity ? identityPresentation(identity) : null;
  const syncMeta = state ? syncPresentation(state) : null;

  return (
    <PageShell
      variant="workspace"
      title="账号中心"
      className="cloud-page page-canvas flex h-full min-h-0 flex-col"
      contentClassName="p-0 flex flex-col flex-1 min-h-0 overflow-hidden"
      actions={identityMeta ? <Badge tone={identityMeta.tone}>{identityMeta.label}</Badge> : undefined}
    >
      <div className="cloud-layout">
        <nav className="cloud-section-nav" aria-label="账号中心分类">
          {SECTIONS.map((item) => {
            const Icon = item.icon;
            return <button key={item.id} type="button" aria-current={section === item.id ? "page" : undefined} onClick={() => setSearchParams(item.id === "account" ? {} : { section: item.id })}>
              <Icon aria-hidden="true" />
              <span><strong>{item.label}</strong></span>
            </button>;
          })}
        </nav>
        <main className="cloud-content scrollbar-thin">
          {loading ? <><Skeleton className="h-28" /><Skeleton className="mt-4 h-56" /></> : null}
          {!loading && !state ? <EmptyState title="账号状态暂时不可用" description="本地核心功能不受影响，请稍后重试。" action={<Button variant="secondary" onClick={() => void refresh()}>重新载入</Button>} /> : null}
          {state && section === "account" ? <section className="cloud-section-stack" aria-labelledby="account-section-title">
            <div className="cloud-section-heading">
              <h2 id="account-section-title">账号与设备</h2>
              <Button size="sm" variant="secondary" onClick={() => void refresh()}><RefreshCw />刷新</Button>
            </div>
            {identity?.status === "guest" && !showSwitchLogin ? (
              <div className="cloud-card cloud-card-title">
                <div><h3>本机工作区</h3><p>无需登录即可使用。登录后可选择跨设备同步。</p></div>
                <Button variant="secondary" onClick={() => setShowSwitchLogin(true)}><LogIn />登录账号</Button>
              </div>
            ) : null}
            {showSwitchLogin ? (
              <div className="cloud-card cloud-auth-card">
                <div className="flex items-center justify-between pb-2">
                  <Tabs
                    variant="segment"
                    size="sm"
                    aria-label="账号操作"
                    tabs={[
                      // Only the active panel is mounted (login/register share one form shell),
                      // so do not emit aria-controls for a panel that is not in the DOM.
                      { id: "login", label: "登录账号", panelId: false },
                      { id: "register", label: "创建账号", panelId: false },
                    ]}
                    activeId={mode}
                    onChange={(val) => setMode(val as "login" | "register")}
                  />
                  {showSwitchLogin && (
                    <Button size="sm" variant="secondary" onClick={() => setShowSwitchLogin(false)}>
                      取消切换
                    </Button>
                  )}
                </div>
                <div id={`panel-${mode}`} role="tabpanel" aria-labelledby={`tab-${mode}`}>
                  <div className="cloud-form-grid">
                    {mode === "register" ? (
                      <Input
                        label="显示名称"
                        value={form.display_name}
                        onChange={(event) => setForm({ ...form, display_name: event.target.value })}
                        autoComplete="name"
                      />
                    ) : null}
                    <Input
                      label="邮箱"
                      type="email"
                      value={form.email}
                      onChange={(event) => setForm({ ...form, email: event.target.value })}
                      autoComplete="email"
                    />
                    <Input
                      label="密码"
                      type="password"
                      value={form.password}
                      onChange={(event) => setForm({ ...form, password: event.target.value })}
                      autoComplete={mode === "login" ? "current-password" : "new-password"}
                    />
                  </div>
                  <details className="cloud-disclosure">
                    <summary>云服务地址</summary>
                    <Input
                      label="服务地址"
                      value={form.cloud_base_url}
                      onChange={(event) => setForm({ ...form, cloud_base_url: event.target.value })}
                    />
                  </details>
                  <div className="cloud-card-actions">
                    <p>登录后可单独选择是否同步本机数据。服务密钥保留在本机。</p>
                    <Button
                      disabled={busy || !form.email.trim() || !form.password}
                      onClick={() => void authenticate()}
                    >
                      <LogIn />
                      {busy ? "正在连接…" : mode === "login" ? "登录并连接" : "创建并连接"}
                    </Button>
                  </div>
                </div>
              </div>
            ) : null}
            {identity?.status === "guest" ? (
              <AccountProfilePanel guestMode identity={identity} lastLogoutReport={lastLogoutReport} />
            ) : (
              <>
                <div className="cloud-card">
                  <div className="cloud-account-summary">
                    <span className="cloud-account-avatar"><UserCircle aria-hidden="true" /></span>
                    <div><h3>{identity?.email || "云端账号"}</h3><p>退出登录会结束本机云会话，云端账号和本地工作数据会保留。</p></div>
                    <Button variant="secondary" disabled={busy} onClick={() => void logout()}><LogOut />退出登录</Button>
                  </div>
                  {state.sync_existing_choice === "undecided" ? (
                    <Button className="mt-3" variant="ghost" size="sm" onClick={() => setSearchParams({ section: "sync" })}>选择是否同步</Button>
                  ) : null}
                </div>
                {sessions.length ? (
                  <div className="cloud-card">
                    <h3>登录设备</h3>
                    <div className="cloud-session-list">
                      {sessions.map((session) => (
                        <article key={session.id}>
                          <div><strong>{session.device_label}</strong><span>最近活动：{new Date(session.last_seen_at).toLocaleString()}</span></div>
                          {session.current ? <Badge tone="info">当前设备</Badge> : session.revoked_at ? <Badge>已退出</Badge> : (
                            <Button size="sm" variant="secondary" disabled={busy} onClick={() => void revokeSession(session.id)}>退出该设备</Button>
                          )}
                        </article>
                      ))}
                    </div>
                  </div>
                ) : null}
                <AccountProfilePanel identity={identity} lastLogoutReport={lastLogoutReport} onRequestLogin={() => { setMode("login"); setShowSwitchLogin(true); }} />
              </>
            )}
          </section> : null}
          {state && syncMeta && section === "sync" ? <section className="cloud-section-stack" aria-labelledby="sync-section-title">
            <div className="cloud-section-heading">
              <h2 id="sync-section-title">同步中心</h2>
              <Button disabled={busy || identity?.status === "guest" || state.sync_existing_choice !== "sync"} onClick={() => void syncNow()}>
                <RefreshCw />{syncing ? "同步中…" : "立即同步"}
              </Button>
            </div>
            <div className="cloud-card">
              <div className="cloud-card-title">
                <div><h3>同步状态</h3><p>{syncMeta.detail}</p></div>
                <Badge tone={syncMeta.tone}>{syncMeta.label}</Badge>
              </div>
              {state.sync.last_error_code ? <p className="cloud-inline-error" role="status">同步遇到错误。本机变更已保留，可稍后重试。</p> : null}
              {state.sync.cursor || state.sync.last_error_code ? (
                <details className="cloud-disclosure">
                  <summary>同步诊断</summary>
                  <dl className="cloud-data-list">
                    {state.sync.last_error_code ? <div><dt>错误代码</dt><dd>{state.sync.last_error_code}</dd></div> : null}
                    {state.sync.cursor ? <div><dt>同步位置</dt><dd>{state.sync.cursor}</dd></div> : null}
                  </dl>
                </details>
              ) : null}
            </div>
            {identity?.status !== "guest" ? (
              <div className="cloud-card">
                <div className="cloud-card-title">
                  <div><h3>跨设备同步</h3><p>启用后会同步已允许的分类。仅本地模式保留现有数据并暂停同步。</p></div>
                  <Button size="sm" variant="ghost" onClick={() => setSearchParams({ section: "privacy" })}>查看同步范围</Button>
                </div>
                <div className="cloud-actions">
                  <Button variant={state.sync_existing_choice === "sync" ? "primary" : "secondary"} aria-pressed={state.sync_existing_choice === "sync"} disabled={busy} onClick={() => void chooseSync("sync")}>启用同步</Button>
                  <Button variant={state.sync_existing_choice === "local_only" ? "primary" : "secondary"} aria-pressed={state.sync_existing_choice === "local_only"} disabled={busy} onClick={() => void chooseSync("local_only")}>保持仅本地</Button>
                </div>
              </div>
            ) : null}
            <div className="cloud-card">
              <ConflictResolutionPanel
                conflicts={conflictItems}
                onResolve={handleResolveConflictItem}
                onRefresh={() => void refreshConflictRecords()}
                isLoading={busy || conflictsLoading}
                loadError={conflictsLoadError}
              />
            </div>
          </section> : null}
          {state && section === "privacy" ? <section className="cloud-section-stack" aria-labelledby="privacy-section-title">
            <div className="cloud-section-heading"><h2 id="privacy-section-title">数据与隐私</h2></div>
            <SyncPrivacyPanel canReadCloudData={state.identity.status !== "guest"} />
            {identity?.status !== "guest" ? (
              <div className="cloud-card cloud-lifecycle-actions">
                <h3>导出与注销</h3>
                <p>可导出云端数据。永久注销会撤销全部会话并删除云端数据，本地工作数据会保留。</p>
                <div className="cloud-actions">
                  <Button variant="secondary" disabled={busy} onClick={() => void requestExport()}>导出我的云端数据</Button>
                  <Button variant="danger" disabled={busy || identity?.status === "closing"} onClick={() => setDeleteDialogOpen(true)}>永久注销云端账号</Button>
                </div>
              </div>
            ) : null}
            {lifecycleJob ? <LifecycleJobsPanel key={lifecycleJob.id} job={lifecycleJob} onRefresh={() => void refreshJob(lifecycleJob.kind, lifecycleJob.id)} isRefreshing={busy} /> : null}
          </section> : null}
          {error ? <p className="cloud-error" role="alert">{error}</p> : null}
        </main>
      </div>
      <ConfirmDialog
        open={deleteDialogOpen}
        title="永久注销云端账号"
        description="此操作将永久删除云端账号及关联会话，但不会删除本地工作数据。"
        confirmLabel="永久注销"
        tone="danger"
        requireText="DELETE"
        onConfirm={confirmDeleteAccount}
        onCancel={() => setDeleteDialogOpen(false)}
      />
    </PageShell>
  );
}
