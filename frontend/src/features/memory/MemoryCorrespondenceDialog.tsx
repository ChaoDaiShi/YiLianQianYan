import { useState } from "react";
import { Button, Input, Modal, Select, Textarea } from "../../components/ui";
import { formatMemoryTime, truncateMemoryContent } from "./memoryCenterModel";
import { changeMemoryLink, createMemoryLink, stageLinkedCorrection, type LinkStatus, type MemoryGraphSnapshot, type MemoryLink } from "./memoryGraphApi";

const statusLabels: Record<LinkStatus, string> = { confirmed: "已确认", pending: "待确认", withdrawn: "已撤回" };
const revisionLabel = (versionId: string) => versionId.slice(versionId.lastIndexOf(":") + 1);

export default function MemoryCorrespondenceDialog({ snapshot, record, selection, onClose, onChanged, onReview }: {
  snapshot: MemoryGraphSnapshot; record: MemoryLink | null; selection: string | null;
  onClose: () => void; onChanged: () => void; onReview: () => void;
}) {
  const [form, setForm] = useState({ a: record?.source_memory_id ?? selection ?? "", b: record?.target_memory_id ?? "", label: record?.label ?? "", status: record?.status ?? "confirmed" as LinkStatus });
  const [correction, setCorrection] = useState(false);
  const [target, setTarget] = useState("");
  const [content, setContent] = useState("");
  const [reason, setReason] = useState("");
  const [relatedVersionId, setRelatedVersionId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const records = snapshot.memories.map(item => ({ value: item.id, label: truncateMemoryContent(item.content, 80) }));
  const history = snapshot.relation_versions.filter(version => record?.id === version.relation_id).reverse();
  const currentVersion = (id: string) => snapshot.histories[id]?.find(version => version.version_id === snapshot.current_version_ids?.[id]);
  const selectable = (exclude: string) => [{ value: "", label: "选择记忆" }, ...records.filter(item => item.value !== exclude)];
  const canSave = correction ? target && content.trim() && reason.trim() && currentVersion(target)
    : form.a && form.b && form.a !== form.b && form.label.trim();

  async function save() {
    if (!canSave || busy) return;
    setBusy(true);
    setError("");
    try {
      if (correction && record) {
        const version = currentVersion(target);
        if (!version) return;
        const result = await stageLinkedCorrection(record.id, {
          target_memory_id: target, content, reason, expected_revision: version.revision,
          source_version_id: relatedVersionId || undefined,
        });
        if (!result.candidate_id) { setError("内容没有变化，或同一纠正已提交。"); return; }
        onReview();
      } else {
        if (record) await changeMemoryLink(record.id, { label: form.label, status: form.status, expected_revision: record.revision });
        else await createMemoryLink({ source_memory_id: form.a, target_memory_id: form.b, label: form.label, status: form.status });
        onChanged();
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  return <Modal open title={correction ? "提议关联记忆纠正" : record ? "对应关系与历史" : "建立记忆对应关系"} size="md"
    onClose={() => { if (!busy) onClose(); }} footer={<>
      <Button variant="secondary" onClick={() => correction ? setCorrection(false) : onClose()} disabled={busy}>{correction ? "返回关系" : "关闭"}</Button>
      <Button loading={busy} disabled={!canSave} onClick={() => void save()}>{correction ? "提交待审阅" : "保存关系"}</Button>
    </>}>
    <div className="memory-network-version-body">
      <p>连线表示两条记忆有对应关系。只有提交并确认具体纠正，才可能改变关联记忆。</p>
      {correction ? <>
        <Select label="要纠正的记忆" value={target} options={[{ value: "", label: "选择记忆" }, ...records.filter(item => item.value === form.a || item.value === form.b)]}
          onChange={event => { setTarget(event.target.value); setContent(currentVersion(event.target.value)?.content ?? ""); setRelatedVersionId(""); }} />
        {target && <Select label="对应的变更（可选）" value={relatedVersionId}
          options={[{value:"",label:"只记录对应关系"},...(snapshot.histories[target === form.a ? form.b : form.a] ?? [])
            .filter(version => version.layer > 0)
            .map(version => ({value:version.version_id,label:`${formatMemoryTime(version.recorded_at)} · 第 ${version.layer} 次更改 · ${truncateMemoryContent(version.content,32)}`}))]}
          onChange={event => setRelatedVersionId(event.target.value)} />}
        <Textarea label="建议的新内容" value={content} maxLength={2000} rows={4} onChange={event => setContent(event.target.value)} />
        <Textarea label="纠正依据" value={reason} maxLength={500} rows={2} onChange={event => setReason(event.target.value)} />
      </> : <>
        <Select label="记忆一" disabled={!!record} value={form.a} options={selectable(form.b)} onChange={event => setForm({ ...form, a: event.target.value })} />
        <Select label="记忆二" disabled={!!record} value={form.b} options={selectable(form.a)} onChange={event => setForm({ ...form, b: event.target.value })} />
        <Input label="关系名称" value={form.label} maxLength={80} onChange={event => setForm({ ...form, label: event.target.value })} />
        <Select label="关系状态" value={form.status} options={Object.entries(statusLabels).filter(([value]) => record || value !== "withdrawn").map(([value, label]) => ({ value, label }))}
          onChange={event => setForm({ ...form, status: event.target.value as LinkStatus })} />
        {record?.status === "confirmed" && <Button size="sm" variant="secondary" onClick={() => { setError(""); setCorrection(true); }}>提议关联记忆纠正</Button>}
        {record && <details><summary>关系历史（{history.length}）</summary><ol>
          {history.map(version => <li key={version.revision}>{`第 ${version.revision} 版 · ${version.label} · ${statusLabels[version.status]} · ${formatMemoryTime(version.recorded_at)}`}
            <div className="memory-semantic-hint">依据为保存时的两端版本 {revisionLabel(version.source_version_id)} / {revisionLabel(version.target_version_id)}</div>
          </li>)}
        </ol></details>}
      </>}
      {error && <p role="alert">{error}</p>}
    </div>
  </Modal>;
}
