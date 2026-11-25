import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { FolderOpen } from "lucide-react";
import { Button, Modal, Select } from "../ui";
import { requestResult } from "../../core/api/http";
import { listWorkspaces, type Workspace } from "../../api/workspaces";

const controlClass = "chat-header-workspace inline-flex items-center gap-1 rounded-lg px-2 py-1.5 text-xs text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]";

export function DefaultWorkspaceLink() {
  const [workspace, setWorkspace] = useState<Workspace>();
  useEffect(() => {
    let active = true;
    void listWorkspaces().then(result => {
      if (active && result.ok) setWorkspace(result.data.find(item => item.is_default));
    });
    return () => { active = false; };
  }, []);
  if (!workspace) return null;
  return <Link className={controlClass} to={`/workspaces/${encodeURIComponent(workspace.id)}`} title="打开默认工作空间" aria-label={workspace.name}>
    <FolderOpen size={14} aria-hidden="true" /><span className="chat-header-action-label max-w-32 truncate">{workspace.name}</span>
  </Link>;
}

export default function ConversationWorkspaceControl({ conversationId }: { conversationId: string }) {
  const [open, setOpen] = useState(false);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspaceId, setWorkspaceId] = useState<string | null>(null);
  const [selected, setSelected] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [refresh, setRefresh] = useState(0);
  const endpoint = `/api/conversations/${encodeURIComponent(conversationId)}/workspace`;
  useEffect(() => {
    let active = true;
    setLoading(true); setError("");
    void Promise.all([requestResult<{ workspace_id: string | null }>("GET", endpoint), listWorkspaces()]).then(([context, choices]) => {
      if (!active) return;
      if (!context.ok || !choices.ok) { setError(!context.ok ? context.error : !choices.ok ? choices.error : ""); return; }
      const boundId = context.data.workspace_id;
      setWorkspaceId(boundId);
      setSelected(choices.data.find(item => item.id === boundId)?.id ?? "");
      setWorkspaces(choices.data);
    }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [endpoint, refresh]);
  async function save() {
    setSaving(true); setError("");
    const response = await requestResult<{ workspace_id: string | null }>("PUT", endpoint, { workspace_id: selected || null });
    setSaving(false);
    if (!response.ok) { setError(response.error); return; }
    setWorkspaceId(response.data.workspace_id); setOpen(false);
  }
  const boundWorkspace = workspaces.find(item => item.id === workspaceId);
  const defaultWorkspace = workspaces.find(item => item.is_default);
  const name = boundWorkspace?.name ?? (workspaceId ? "原工作空间已移除" : "工作空间");
  return <>
    <button type="button" className={controlClass} title="选择当前对话工作空间" aria-label={name} onClick={() => { setSelected(boundWorkspace?.id ?? ""); setOpen(true); setRefresh(current => current + 1); }}><FolderOpen size={14} aria-hidden="true" /><span className="chat-header-action-label max-w-32 truncate">{name}</span></button>
    <Modal open={open} onClose={() => { if (!saving) setOpen(false); }} title="对话工作空间" size="md" footer={<><Button variant="secondary" disabled={saving} onClick={() => setOpen(false)}>取消</Button><Button loading={saving} disabled={loading || saving || Boolean(error)} onClick={() => void save()}>保存选择</Button></>}>
      <div className="space-y-4">
        {error && <div role="alert" className="text-sm text-[var(--danger)]">{error}<Button size="sm" variant="ghost" onClick={() => setRefresh(current => current + 1)}>重新读取</Button></div>}
        {loading ? <p role="status">正在读取工作空间…</p> : <Select label="当前对话的工作空间" value={selected} onChange={event => setSelected(event.target.value)} options={[{value:"",label: defaultWorkspace ? `使用默认空间 · ${defaultWorkspace.name}` : "使用默认工作空间"}, ...workspaces.map(item => ({value:item.id,label:`${item.name}${item.status === "archived" ? "（已归档）" : ""}`,disabled:item.status === "archived"}))]} />}
        <p className="text-xs leading-relaxed text-[var(--text-secondary)]">用于后续对话上下文和项目记忆范围，保存后从下一轮生效。</p>
      </div>
    </Modal>
  </>;
}
