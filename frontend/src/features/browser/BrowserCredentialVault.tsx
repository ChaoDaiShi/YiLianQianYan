import { useEffect, useState, type FormEvent } from "react";
import { Button, FormField, Input, Modal } from "../../components/ui";

interface Entry { id: string; label: string; origin: string; authorized: boolean }
interface Draft { id?: string; label: string; origin: string; username: string; password: string }
type Request = { action: "list" } | { action: "save"; entry: Draft }
  | { action: "authorize"; id: string; authorized: boolean } | { action: "delete"; id: string };

async function invokeVault(request: Request) {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<Entry[]>("native_browser_credential_vault", { request });
}

export default function BrowserCredentialVault({ origin, onClose }: { origin: string; onClose: () => void }) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [confirmation, setConfirmation] = useState<{ entry: Entry; action: "authorize" | "delete" } | null>(null);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");

  async function send(request: Request) {
    setBusy(true);
    setError("");
    try {
      setEntries(await invokeVault(request));
      setDraft(null);
      setConfirmation(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => { void send({ action: "list" }); }, []);

  function edit(entry?: Entry) {
    setConfirmation(null);
    setError("");
    setDraft({ id: entry?.id, label: entry?.label ?? "", origin: entry?.origin ?? origin, username: "", password: "" });
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    if (draft && !busy) void send({ action: "save", entry: draft });
  }

  return <Modal open title="密码箱" onClose={() => { if (!busy) onClose(); }}>
    <div className="space-y-4">
      <p className="text-sm text-[var(--text-secondary)]">保存后默认未授权。授权仅允许 AI 向该网站的登录表单填充凭据，登录由你提交。</p>
      {error && <p role="alert" className="break-words text-sm text-[var(--danger-fg)]">{error}</p>}
      {busy && <p role="status" className="text-sm">正在处理…</p>}
      {!busy && !entries.length && !draft && <p className="text-sm">还没有保存凭据。</p>}
      <ul className="space-y-2" aria-label="已保存的网站凭据">
        {entries.map(entry => <li key={entry.id} className="flex flex-wrap items-center justify-between gap-2 rounded-[var(--radius-md)] border border-[var(--border-soft)] p-3">
          <div className="min-w-0 break-words">
            <strong className="text-sm">{entry.label}</strong>
            <p className="text-xs text-[var(--text-secondary)]">{entry.origin} · {entry.authorized ? "已授权" : "未授权"}</p>
          </div>
          <div className="flex flex-wrap gap-1">
            <Button size="sm" variant="secondary" disabled={busy} onClick={() => {
              setDraft(null);
              if (entry.authorized) void send({ action: "authorize", id: entry.id, authorized: false });
              else setConfirmation({ entry, action: "authorize" });
            }}>{entry.authorized ? "撤销授权" : "授权填充"}</Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => edit(entry)}>更换凭据</Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => { setDraft(null); setConfirmation({ entry, action: "delete" }); }}>删除</Button>
          </div>
        </li>)}
      </ul>
      {confirmation && <section className="space-y-2 rounded-[var(--radius-md)] border border-[var(--border-soft)] p-3" aria-label="确认密码箱操作">
        <p className="break-words text-sm">{confirmation.action === "authorize"
          ? `允许 AI 使用「${confirmation.entry.label}」填充 ${confirmation.entry.origin} 的登录表单？`
          : `删除「${confirmation.entry.label}」并撤销其网站授权？`}</p>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" disabled={busy} onClick={() => void send(confirmation.action === "authorize"
            ? { action: "authorize", id: confirmation.entry.id, authorized: true }
            : { action: "delete", id: confirmation.entry.id })}>{confirmation.action === "authorize" ? "确认授权" : "确认删除"}</Button>
          <Button size="sm" variant="secondary" disabled={busy} onClick={() => setConfirmation(null)}>取消</Button>
        </div>
      </section>}
      {draft ? <form className="space-y-3 border-t border-[var(--border-soft)] pt-3" onSubmit={submit}>
        <FormField label="条目名称" required>{props => <Input {...props} required maxLength={100} value={draft.label} disabled={busy} onChange={e => setDraft({ ...draft, label: e.target.value })} />}</FormField>
        <FormField label="网站地址" required>{props => <Input {...props} type="url" required placeholder="https://example.com" value={draft.origin} disabled={busy} onChange={e => setDraft({ ...draft, origin: e.target.value })} />}</FormField>
        <FormField label="账号" required>{props => <Input {...props} required autoComplete="off" value={draft.username} disabled={busy} onChange={e => setDraft({ ...draft, username: e.target.value })} />}</FormField>
        <FormField label="密码" required>{props => <Input {...props} type="password" required autoComplete="new-password" value={draft.password} disabled={busy} onChange={e => setDraft({ ...draft, password: e.target.value })} />}</FormField>
        {draft.id && <p className="text-xs text-[var(--text-secondary)]">更换凭据后需要重新授权。</p>}
        <div className="flex flex-wrap gap-2">
          <Button type="submit" size="sm" disabled={busy}>保存凭据</Button>
          <Button size="sm" variant="secondary" disabled={busy} onClick={() => setDraft(null)}>取消</Button>
        </div>
      </form> : <Button size="sm" disabled={busy} onClick={() => edit()}>新增凭据</Button>}
    </div>
  </Modal>;
}
