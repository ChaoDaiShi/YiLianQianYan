import { useEffect, useState } from "react";
import { Badge, Button, Textarea } from "../../components/ui";
import { correctMemory, getMemoryGovernance, getMemoryHistory, withdrawMemory, type MemoryGovernance, type MemoryRecord, type MemoryVersion } from "../../api/memory";
import { listWorkspaces, type Workspace } from "../../api/workspaces";

export function MemoryGovernancePanel({ memory, onChanged }: { memory: MemoryRecord; onChanged: () => void }) {
  const [governance, setGovernance] = useState<MemoryGovernance | null>(null);
  const [versions, setVersions] = useState<MemoryVersion[]>([]);
  const [content, setContent] = useState(memory.content);
  const [reason, setReason] = useState("");
  const [scope, setScope] = useState("profile");
  const [sourceRef, setSourceRef] = useState("");
  const [editing, setEditing] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refresh, setRefresh] = useState(0);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  useEffect(() => {
    let active = true;
    listWorkspaces().then((result) => {
      if (!active) return;
      if (result.ok) setWorkspaces(result.data.filter((workspace) => workspace.status === "active"));
      else setError(result.error);
    });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    let active = true;
    setGovernance(null); setVersions([]); setEditing(false); setContent(memory.content); setReason(""); setError(null);
    Promise.all([getMemoryGovernance(memory.id), getMemoryHistory(memory.id)]).then(([record, history]) => {
      if (!active) return;
      setGovernance(record); setVersions(history); setScope(record.scope); setSourceRef(record.source_ref ?? "");
    }).catch((cause: unknown) => { if (active) setError(cause instanceof Error ? cause.message : String(cause)); });
    return () => { active = false; };
  }, [memory.id, memory.updated_at, refresh]);

  const save = async (action: "correct" | "withdraw") => {
    if (!governance || pending || !reason.trim()) return;
    setPending(true); setError(null);
    try {
      if (action === "correct") {
        await correctMemory(memory.id, { content: content.trim(), expected_revision: governance.revision, reason: reason.trim(), scope, source_ref: sourceRef.trim() || undefined });
      } else {
        await withdrawMemory(memory.id, { expected_revision: governance.revision, reason: reason.trim() });
      }
      setRefresh((value) => value + 1); onChanged();
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setPending(false); }
  };

  return <section className="space-y-3 border-t border-[var(--border-soft)] pt-4" aria-label="记忆版本与来源">
    <div className="flex items-center justify-between gap-2">
      <h3 className="text-sm font-medium">版本与采用范围</h3>
      {governance && <Badge tone={governance.status === "active" ? "success" : "warning"}>v{governance.revision} · {governance.status === "active" ? "当前可用" : "已撤回"}</Badge>}
    </div>
    {error && <p role="alert" className="text-xs text-[var(--danger)]">{error}<button className="ml-2 underline" onClick={() => setRefresh((value) => value + 1)}>重新加载</button></p>}
    {governance && <>
      <dl className="memory-detail-fields">
        <div><dt>采用范围</dt><dd>{governance.scope === "profile" ? "当前档案" : governance.scope}</dd></div>
        <div><dt>来源引用</dt><dd>{governance.source_ref || "未提供来源引用"}</dd></div>
        <div><dt>事实有效时间</dt><dd>{governance.valid_from ? new Date(governance.valid_from).toLocaleString("zh-CN") : "未知；入库时间不代表事实发生时间"}</dd></div>
        <div><dt>检索索引</dt><dd>{governance.embedding_revision === governance.revision ? `${governance.embedding_model} · v${governance.embedding_revision}` : "待重建"}</dd></div>
      </dl>
      {governance.status === "active" && <Button variant="secondary" size="sm" onClick={() => setEditing(!editing)}>{editing ? "取消纠正" : "纠正或撤回"}</Button>}
      {editing && <div className="space-y-3 rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-3">
        <Textarea label="纠正后的内容" value={content} onChange={(event) => setContent(event.target.value)} />
        <Textarea label="变更原因（必填）" value={reason} onChange={(event) => setReason(event.target.value)} placeholder="说明为什么需要纠正或撤回" />
        <label className="block text-xs">采用范围
          <select className="mt-1 w-full rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2" value={scope} onChange={(event) => setScope(event.target.value)}>
            <option value="profile">当前档案内使用</option>
            {workspaces.map((workspace) => <option key={workspace.id} value={`workspace:${workspace.id}`}>仅项目：{workspace.name}</option>)}
            {memory.source_conversation_id && <option value={`conversation:${memory.source_conversation_id}`}>仅来源对话</option>}
            {scope !== "profile" && scope !== `conversation:${memory.source_conversation_id}` && <option value={scope}>{scope}</option>}
          </select>
        </label>
        <Textarea label="来源引用" value={sourceRef} onChange={(event) => setSourceRef(event.target.value)} placeholder="来源对话或证据说明" />
        <p className="text-xs text-[var(--text-secondary)]">纠正保留旧版本。撤回后不再被检索采用，历史仍可查阅；删除会同时移除历史。</p>
        <div className="flex gap-2">
          <Button size="sm" disabled={pending || !reason.trim() || !content.trim()} onClick={() => void save("correct")}>保存纠正</Button>
          <Button variant="secondary" size="sm" disabled={pending || !reason.trim()} onClick={() => void save("withdraw")}>撤回记忆</Button>
        </div>
      </div>}
    </>}
    <details className="memory-technical-details">
      <summary>历史版本（{versions.length}）</summary>
      <ol className="mt-3 space-y-3">
        {versions.map((version) => <li key={version.revision} className="rounded border border-[var(--border-soft)] p-3 text-xs">
          <p className="font-medium">v{version.revision} · {version.status === "active" ? "可用" : "已撤回"} · {new Date(version.recorded_at).toLocaleString("zh-CN")}</p>
          <p className="mt-2 whitespace-pre-wrap">{version.content}</p>
          <p className="mt-2 text-[var(--text-secondary)]">原因：{version.reason} · 范围：{version.scope} · 来源：{version.source_ref || version.source}</p>
        </li>)}
      </ol>
    </details>
  </section>;
}
