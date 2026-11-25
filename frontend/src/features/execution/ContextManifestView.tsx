import { useEffect, useState } from "react";
import { getConversationContextManifest, getWorkflowContextManifest, type ContextManifest } from "../../api/contextManifest";
import { Button, EmptyState, ErrorState } from "../../components/ui";
const labels: Record<string, string> = { resolved: "已解析", truncated: "已截断", unavailable: "不可用", unsupported: "当前执行器未支持", excluded: "已排除" };
export function ContextManifestView({ manifest }: { manifest: ContextManifest }) {
  return <section className="min-w-0 text-sm" aria-label="执行上下文来源"><p className="mb-3 text-xs text-[var(--text-secondary)]">本轮上下文组装记录（历史快照），记录当时解析的输入来源与裁剪结果。引用不授予权限，后续执行仍重新检查当前授权；这里不显示输入原文或凭据。共 {manifest.total_chars} 字符。</p>{manifest.entries.length === 0 ? <EmptyState title="没有记录额外上下文来源" description="此清单只描述已记录的上下文，不代表自动读取整个工作空间。" /> : <ul className="space-y-2">{manifest.entries.map((entry, index) => <li key={`${entry.kind}:${entry.source_id}:${index}`} className="rounded border border-[var(--border-soft)] p-3"><div className="flex flex-wrap justify-between gap-2"><strong>{entry.kind}</strong><span>{labels[entry.status] || entry.status}</span></div><p className="mt-1 break-all text-xs">{entry.source_id}{entry.version ? ` · 版本 ${entry.version}` : ""} · {entry.chars} 字符</p>{entry.reason && <p className="mt-1 break-words text-xs text-[var(--text-secondary)]">{entry.reason}</p>}</li>)}</ul>}</section>;
}
export default function ConversationContextManifest({ conversationId }: { conversationId: string }) {
  const [manifest, setManifest] = useState<ContextManifest | null>(null);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  useEffect(() => { let active = true; setManifest(null); setError(""); void getConversationContextManifest(conversationId).then(value => { if (active) setManifest(value.manifest); }).catch(cause => { if (active) setError(cause instanceof Error ? cause.message : String(cause)); }); return () => { active = false; }; }, [conversationId, revision]);
  return error ? <ErrorState title="上下文来源暂不可用" description={error} action={<Button size="sm" variant="secondary" onClick={() => setRevision(value => value + 1)}>重新读取</Button>} /> : manifest ? <ContextManifestView manifest={manifest} /> : <p role="status">正在读取最近一次执行的上下文来源…</p>;
}

/** Inline content: the workflow inspector already owns the surrounding dialog. */
export function WorkflowContextManifest({ runId }: { runId: string }) {
  const [manifest, setManifest] = useState<ContextManifest | null>(null);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  useEffect(() => { let active = true; setManifest(null); setError(""); void getWorkflowContextManifest(runId).then(value => { if (active) setManifest(value.manifest); }).catch(cause => { if (active) setError(cause instanceof Error ? cause.message : String(cause)); }); return () => { active = false; }; }, [runId, revision]);
  return error ? <ErrorState title="工作流上下文来源暂不可用" description={error} action={<Button size="sm" variant="secondary" onClick={() => setRevision(value => value + 1)}>重新读取</Button>} /> : manifest ? <ContextManifestView manifest={manifest} /> : <p role="status">正在读取本次工作流实际组装的上下文来源…</p>;
}
