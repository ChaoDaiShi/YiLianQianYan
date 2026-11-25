import { useEffect, useRef, useState } from "react";
import { ArrowLeft, Download, ExternalLink, FileText, RefreshCw } from "lucide-react";
import { Link } from "react-router-dom";
import { listOutputs, previewOutput, downloadOutput, type OutputRef, type OutputPreview, type OutputCatalogue } from "../../api/outputs";
import { Button, EmptyState, ErrorState, Modal } from "../../components/ui";
import { formatTaskDate } from "../tasks/taskPresentation";
import { workStatusLabels } from "../tasks/workIndexPresentation";

const sourceLabels: Record<string, string> = { artifact: "任务交付物", conversation: "对话成果", personal_file: "个人助手文件" };
const verificationLabels: Record<string, string> = { passed: "核验通过", accepted: "核验通过", failed: "核验失败", pending: "待核验", not_requested: "未请求核验", unknown: "核验状态未知", resource_integrity_checked_on_read: "读取时检查资源完整性", not_applicable: "不适用", verified: "核验通过" };
export default function UnifiedOutputs({ workspaceId, limit = 50, embeddedPreview = false, title }: { workspaceId?: string; limit?: number; embeddedPreview?: boolean; title?: string }) {
  const [outputs, setOutputs] = useState<OutputRef[] | null>(null);
  const [issues, setIssues] = useState<NonNullable<OutputCatalogue["issues"]>>([]);
  const [truncated, setTruncated] = useState(false);
  const [error, setError] = useState("");
  const [selected, setSelected] = useState<OutputRef | null>(null);
  const [revision, setRevision] = useState(0);
  const previousScope = useRef(workspaceId);
  useEffect(() => {
    let active = true;
    if (previousScope.current !== workspaceId) { setOutputs(null); setSelected(null); setIssues([]); previousScope.current = workspaceId; }
    void listOutputs(workspaceId, limit).then(result => { if (active) { setOutputs(result.outputs); setTruncated(result.truncated); setIssues(result.issues ?? []); setError(""); } }).catch(cause => { if (active) setError(cause instanceof Error ? cause.message : String(cause)); });
    return () => { active = false; };
  }, [workspaceId, limit, revision]);
  if (selected && embeddedPreview) return <section className="min-w-0" aria-label="统一成果预览"><Button variant="secondary" size="sm" onClick={() => setSelected(null)}><ArrowLeft size={14} />返回成果列表</Button><OutputPreviewContent key={`${selected.kind}:${selected.id}:${selected.source.id}`} output={selected} workspaceId={workspaceId} onClose={() => setSelected(null)} /></section>;
  return <section className="workspace-artifacts min-w-0" aria-label={workspaceId ? "工作空间统一成果" : "统一成果列表"}>
    <div className="workspace-section-heading"><h2>{title ?? (workspaceId ? "空间成果" : "全部成果")}</h2><Button variant="ghost" size="sm" iconOnly aria-label="刷新成果" title="刷新成果" onClick={() => setRevision(value => value + 1)}><RefreshCw size={15} /></Button></div>
    <p className="mb-3 text-xs text-[var(--text-secondary)]">汇总任务、对话和个人助手的真实成果。来源记录、文件是否写入与交付核验分别保留；预览时重新检查当前权限和文件。</p>
    {error && <ErrorState title="成果列表读取失败" description={`${error}${outputs ? "；保留上次结果，可能不是最新状态。" : ""}`} onRetry={() => setRevision(value => value + 1)} />}
    {issues.length > 0 && <div role="status" className="mb-3 rounded border border-[var(--warning-border)] p-3 text-xs"><p>部分成果来源需核实（{issues.length} 项），有效成果仍保留在下方。</p><ul className="my-2 space-y-1">{issues.slice(0, 5).map((issue, index) => <li key={`${issue.kind}:${issue.id}:${index}`} className="break-words">{issue.kind} · {Array.from(issue.id).slice(0, 80).join("")}：{Array.from(issue.error).slice(0, 240).join("")}{Array.from(issue.error).length > 240 ? "…" : ""}</li>)}</ul>{issues.length > 5 && <p>另有 {issues.length - 5} 项诊断未展开。</p>}<Button size="sm" variant="secondary" onClick={() => setRevision(value => value + 1)}>重新读取成果</Button></div>}
    {!outputs ? !error && <p className="workspace-tree-note" role="status">正在读取成果…</p> : outputs.length === 0 ? <EmptyState icon={<FileText size={22} />} title={issues.length ? "没有可显示的成果" : title ? "还没有任务或对话成果" : "还没有保存的成果"} description={issues.length ? "部分来源不可读取，请核实上方诊断后重新读取；此状态不代表所有历史成果均不存在。" : "对话成果、已生成的个人助手文件和任务交付物会按来源显示在这里。"} className="workspace-artifacts-empty" /> : <div className="workspace-folder-list">
      {outputs.map(output => <article key={`${output.kind}:${output.id}:${output.source.id}`} data-output-kind={output.kind} className="workspace-folder-row workspace-artifact-row min-w-0 flex-wrap">
        <span className="workspace-result-icon"><FileText size={25} /></span>
        <div className="workspace-row-copy min-w-0"><h3 className="break-words">{output.title}</h3><p>{sourceLabels[output.kind] ?? output.kind} · {formatTaskDate(output.updated_at || output.created_at)}</p><p>{workStatusLabels[output.execution_status] ?? output.execution_status} · {verificationLabels[output.verification_status] ?? output.verification_status}</p>{output.source_workspace_id !== output.destination_workspace_id && <p className="break-all">来源空间：{output.source_workspace_id || "未绑定"} · 导出空间：{output.destination_workspace_id || "未指定"}</p>}</div>
        <div className="flex flex-wrap gap-2"><Button variant="secondary" size="sm" onClick={() => setSelected(output)}>查看成果</Button><Link to={output.source_href} className="inline-flex items-center gap-1 text-xs text-[var(--accent-soft-fg)]">打开来源<ExternalLink size={13} /></Link></div>
      </article>)}
    </div>}
    {truncated && <p role="status" className="workspace-tree-note">当前达到 {limit} 项上限，请按空间查看或打开来源读取更早记录。</p>}
    {selected && <Modal open title={selected.title} size="xl" onClose={() => setSelected(null)}><OutputPreviewContent key={`${selected.kind}:${selected.id}:${selected.source.id}`} output={selected} workspaceId={workspaceId} onClose={() => setSelected(null)} /></Modal>}
  </section>;
}

function OutputPreviewContent({ output, workspaceId, onClose }: { output: OutputRef; workspaceId?: string; onClose: () => void }) {
  const [preview, setPreview] = useState<OutputPreview | null>(null);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  const [downloading, setDownloading] = useState(false);
  useEffect(() => {
    let active = true; setPreview(null); setError("");
    void previewOutput(output, workspaceId).then(result => { if (active) setPreview(result); }).catch(cause => { if (active) setError(cause instanceof Error ? cause.message : String(cause)); });
    return () => { active = false; };
  }, [output, workspaceId, revision]);
  async function download() {
    setDownloading(true); setError("");
    try { const bytes = await downloadOutput(output, workspaceId); const url = URL.createObjectURL(bytes); const link = document.createElement("a"); link.href = url; link.download = output.title; link.click(); window.setTimeout(() => URL.revokeObjectURL(url), 1000); }
    catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setDownloading(false); }
  }
  return <div className="workspace-artifact-preview min-w-0 pt-3" data-testid="unified-output-preview">
    <div className="workspace-artifact-source flex-wrap"><strong className="break-words">{output.title}</strong><span>{sourceLabels[output.kind]}</span><Link to={output.source_href} onClick={onClose}>打开原始记录<ExternalLink size={13} /></Link></div>
    {error ? <ErrorState title="成果暂时无法预览" description={error} action={<Button size="sm" variant="secondary" onClick={() => setRevision(value => value + 1)}>重新检查权限与文件</Button>} /> : !preview ? <p role="status">正在核对来源与当前读取权限…</p> : <>
      {preview.text !== null ? <pre className="workspace-text-preview whitespace-pre-wrap break-words" style={{ overflowWrap: "anywhere" }}>{preview.text}</pre> : <p className="workspace-tree-note">此成果没有可用的文本预览，请打开原始记录查看支持的格式。{preview.resource_preview?.status === "unsupported" ? "当前格式不支持预览。" : preview.resource_preview?.status === "ready" ? "资源读取与完整性已检查。" : ""}</p>}
      {preview.truncated && <p className="workspace-tree-note">内容超过预览上限，当前只显示部分文本。</p>}
      {output.download_url && <Button variant="secondary" size="sm" loading={downloading} onClick={() => void download()}><Download size={15} />下载原文件</Button>}
    </>}
    <details className="workspace-source-details break-words"><summary>来源与核验</summary><p>执行状态：{workStatusLabels[output.execution_status] ?? output.execution_status}</p><p>核验状态：{verificationLabels[output.verification_status] ?? output.verification_status}</p><p>来源空间：{output.source_workspace_id || "未绑定"}</p><p>导出空间：{output.destination_workspace_id || "未指定"}</p><pre className="whitespace-pre-wrap break-all">{JSON.stringify(output.source, null, 2)}</pre></details>
  </div>;
}
