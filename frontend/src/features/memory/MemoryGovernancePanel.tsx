import { useEffect, useState } from "react";
import { Badge, Button, Textarea } from "../../components/ui";
import { correctMemory, getMemoryGovernance, getMemoryHistory, withdrawMemory, type MemoryGovernance, type MemoryRecord, type MemoryVersion } from "../../api/memory";
import { listWorkspaces, type Workspace } from "../../api/workspaces";
import { useMemoryStore } from "../../stores/memoryStore";

export function MemoryGovernancePanel({ memory, onChanged, readOnly = false, graphHistory }: { memory: MemoryRecord; onChanged: () => void; readOnly?: boolean; graphHistory?: MemoryVersion[] }) {
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
    Promise.all([getMemoryGovernance(memory.id), graphHistory ?? getMemoryHistory(memory.id)]).then(([record, history]) => {
      if (!active) return;
      setGovernance(record); setVersions(history); setScope(record.scope); setSourceRef(record.source_ref ?? "");
    }).catch((cause: unknown) => { if (active) setError(cause instanceof Error ? cause.message : String(cause)); });
    return () => { active = false; };
  }, [memory.id, memory.updated_at, refresh, graphHistory]);

  const save = async (action: "correct" | "withdraw") => {
    if (!governance || pending || !reason.trim()) return;
    setPending(true); setError(null);
    try {
      if (action === "correct") {
        await correctMemory(memory.id, { content: content.trim(), expected_revision: governance.revision, reason: reason.trim(), scope, source_ref: sourceRef.trim() || undefined });
      } else {
        await withdrawMemory(memory.id, { expected_revision: governance.revision, reason: reason.trim() });
        useMemoryStore.getState().evictMemory(memory.id);
      }
      setRefresh((value) => value + 1); onChanged();
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setPending(false); }
  };

  const workspaceId = governance?.scope.startsWith("workspace:") ? governance.scope.slice("workspace:".length) : null;
  const workspace = workspaceId ? workspaces.find(item => item.id === workspaceId) : null;
  const scopeLabel = workspaceId ? `工作空间${workspace ? ` · ${workspace.name}` : ""}`
    : governance?.scope.startsWith("conversation:") ? "来源对话" : "当前档案";

  return <section className="space-y-3 border-t border-[var(--border-soft)] pt-4" aria-label="记忆版本与来源">
    <div className="flex items-center justify-between gap-2">
      <h3 className="text-sm font-medium">版本与采用范围</h3>
      {governance && <Badge tone={governance.status === "active" ? "success" : "warning"}>v{governance.revision} · {governance.status === "active" ? "当前可用" : "已撤回"}</Badge>}
    </div>
    {error && <p role="alert" className="text-xs text-[var(--danger)]">{error}<button className="ml-2 underline" onClick={() => setRefresh((value) => value + 1)}>重新加载</button></p>}
    {governance && <>
      <dl className="memory-detail-fields">
        <div><dt>采用范围</dt><dd>{scopeLabel}</dd></div>
        {!readOnly && <>
        <div><dt>来源引用</dt><dd>{memory.source_removed ? "来源对话已移除" : governance.source_ref || "未提供来源引用"}</dd></div>
        <div><dt>事实有效时间</dt><dd>{governance.valid_from ? new Date(governance.valid_from).toLocaleString("zh-CN") : "未知；入库时间不代表事实发生时间"}</dd></div>
        <div><dt>检索索引</dt><dd>{governance.embedding_revision === governance.revision ? `${governance.embedding_model} · v${governance.embedding_revision}` : "待重建"}</dd></div>
        </>}
      </dl>
      {!readOnly && governance.status === "active" && <Button variant="secondary" size="sm" onClick={() => setEditing(!editing)}>{editing ? "取消纠正" : "纠正或撤回"}</Button>}
      {!readOnly && editing && <div className="space-y-3 rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-3">
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
        <p className="text-xs text-[var(--text-secondary)]">纠正保留旧版本。撤回会停止在回答中采用这条记忆，并保留正文与历史；永久清除会移除正文、历史和检索索引。</p>
        <div className="flex gap-2">
          <Button size="sm" disabled={pending || !reason.trim() || !content.trim()} onClick={() => void save("correct")}>保存纠正</Button>
          <Button variant="secondary" size="sm" disabled={pending || !reason.trim()} onClick={() => void save("withdraw")}>撤回记忆</Button>
        </div>
      </div>}
    </>}
    {(!readOnly || versions.length > 1) && <details className="memory-technical-details">
      <summary>历史版本（{versions.length}）</summary>
      <ol className="memory-version-stack mt-3 space-y-3">
        {versions.map((version) => <li key={version.revision} className="rounded border border-[var(--border-soft)] p-3 text-xs">
          <p className="font-medium">{version.layer === 0 ? version.baseline ? "已知基线" : "原始层" : `第 ${version.layer} 层`} · {version.status === "active" ? "记录时有效" : "已撤回"} · {new Date(version.recorded_at).toLocaleString("zh-CN")}</p>
          <p className="mt-2 whitespace-pre-wrap">{version.content}</p>
          <p className="mt-2 text-[var(--text-secondary)]">原因：{version.baseline ? "更早记录未知" : version.reason === "created" ? "创建记忆" : version.reason} · 范围：{version.scope} · 来源：{version.source_removed ? "来源已移除" : version.source_ref || version.source}</p>
        </li>)}
      </ol>
    </details>}
  </section>;
}
