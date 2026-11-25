import { useEffect, useState } from "react";
import { FilePlus2 } from "lucide-react";
import { Link } from "react-router-dom";
import { Button, Input, Modal, Select } from "../ui";
import PagePicker from "../../features/workspace-preview/PagePicker";
import { listWorkspaces, type Workspace } from "../../api/workspaces";
import { saveAnswerToPage } from "../../api/richContent";
import { loadPage, workspacePagePath, type PageDocument, type WorkspacePage } from "../../api/workspacePages";
import { requestResult } from "../../core/api/http";
import "../../features/workspace-preview/workspacePages.css";

export default function SaveAnswerToPage({ conversation, message, disabled, imageIds }: { conversation: string; message: string; disabled: boolean; imageIds?: string[] }) {
  const [open, setOpen] = useState(false);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspace, setWorkspace] = useState("");
  const [title, setTitle] = useState("聊天成果");
  const [target, setTarget] = useState<WorkspacePage>();
  const [mode, setMode] = useState<"new" | "child" | "append">("new");
  const [saved, setSaved] = useState<PageDocument>();
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!open) return;
    let current = true;
    setLoading(true);
    void Promise.all([
      listWorkspaces(),
      requestResult<{ workspace_id: string | null }>("GET", "/api/conversations/" + encodeURIComponent(conversation) + "/workspace"),
    ]).then(([choices, context]) => {
      if (!current) return;
      setLoading(false);
      if (!choices.ok) { setError(choices.error); return; }
      const active = choices.data.filter(option => option.status === "active");
      setWorkspaces(active);
      setWorkspace(active.find(option => context.ok && option.id === context.data.workspace_id)?.id ??
        active.find(option => option.is_default)?.id ?? active[0]?.id ?? "");
    });
    return () => { current = false; };
  }, [conversation, open]);

  const save = async () => {
    setSaving(true); setError("");
    const latest = mode === "append" && target ? await loadPage(target.id) : null;
    if (latest && !latest.ok) { setSaving(false); setError(latest.error); return; }
    const result = await saveAnswerToPage(conversation, message, {
      workspace_id: workspace,
      title: title.trim(),
      parent_page_id: mode === "child" ? target?.id : undefined,
      page_id: mode === "append" ? target?.id : undefined,
      revision: latest?.ok ? latest.data.revision : undefined,
      image_ids: imageIds,
    });
    setSaving(false);
    if (result.ok) setSaved(result.data);
    else setError(result.error);
  };

  return <>
    <button type="button" disabled={disabled} className="rich-save-answer" onClick={() => {
      setError(""); setSaved(undefined); setTarget(undefined); setOpen(true);
    }}><FilePlus2 size={14} aria-hidden="true" />保存到页面</button>
    <Modal open={open} title="保存聊天成果" size="md" closeDisabled={saving} onClose={() => setOpen(false)}
      footer={saved ? <Button onClick={() => setOpen(false)}>完成</Button> : <>
        <Button variant="secondary" disabled={saving} onClick={() => setOpen(false)}>取消</Button>
        <Button loading={saving} disabled={loading || !workspace || (mode === "append" ? !target : !title.trim() || (mode === "child" && !target))}
          onClick={() => void save()}>保存成果</Button>
      </>}>
      {saved ? <p role="status">已保存。 <Link className="rich-save-link" to={workspacePagePath(saved.workspace_id, saved.id)}>打开{saved.title}</Link></p> :
        <div className="rich-form">
          {error && <p role="alert" className="rich-component-error">{error}</p>}
          {loading ? <p role="status">正在读取工作空间…</p> : !workspaces.length ? <p>没有可以写入的工作空间。</p> : <>
            <Select label="工作空间" value={workspace} disabled={saving} onChange={event => { setWorkspace(event.target.value); setTarget(undefined); }}
              options={workspaces.map(option => ({ value: option.id, label: option.name }))} />
            <Select label="保存位置" value={mode} disabled={saving} onChange={event => { setMode(event.target.value as typeof mode); setTarget(undefined); }}
              options={[{value:"new",label:"创建根页面"},{value:"child",label:"创建子页面"},{value:"append",label:"追加到已有页面"}]} />
            {mode !== "append" && <Input label="页面标题" value={title} maxLength={200} disabled={saving} onChange={event => setTitle(event.target.value)} />}
            {mode !== "new" && <PagePicker key={workspace} workspace={workspace} selected={target?.id} onSelect={setTarget} />}
          </>}
        </div>}
    </Modal>
  </>;
}
