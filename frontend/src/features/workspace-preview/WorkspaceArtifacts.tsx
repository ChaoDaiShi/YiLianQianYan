import { useEffect, useState } from "react";
import { Download, ExternalLink, FileText, Image, RefreshCw } from "lucide-react";
import { Link } from "react-router-dom";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { downloadArtifact, listWorkspaceArtifacts, previewArtifact, type ArtifactPreview, type ArtifactRecord } from "../../api/evidence";
import { Button, EmptyState, ErrorState, Modal } from "../../components/ui";
import { formatTaskDate } from "../tasks/taskPresentation";

export default function WorkspaceArtifacts({ workspaceId, limit = 30 }: { workspaceId?: string; limit?: number }) {
  const [artifacts, setArtifacts] = useState<ArtifactRecord[] | null>(null);
  const [error, setError] = useState("");
  const [selected, setSelected] = useState<ArtifactRecord | null>(null);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let active = true;
    setArtifacts(null); setError("");
    void listWorkspaceArtifacts(workspaceId, limit).then(result => { if (active) setArtifacts(result); }).catch(cause => { if (active) setError(cause instanceof Error ? cause.message : "成果列表读取失败"); });
    return () => { active = false; };
  }, [workspaceId, limit, revision]);
  return <section className="workspace-artifacts" aria-label={workspaceId ? "工作空间成果" : "最近成果"}>
    <div className="workspace-section-heading"><h2>{workspaceId ? "空间成果" : "最近成果"}</h2><Button variant="ghost" size="sm" iconOnly aria-label="刷新成果" title="刷新成果" onClick={() => setRevision(value => value + 1)}><RefreshCw size={15} /></Button></div>
    {error ? <ErrorState title="成果暂时无法加载" description={error} onRetry={() => setRevision(value => value + 1)} /> : artifacts === null ? <p className="workspace-tree-note" role="status">正在读取成果…</p> : artifacts.length === 0 ? <EmptyState icon={<FileText size={22} />} title="还没有保存的成果" description="任务生成并保存的文稿、图片和文档会显示在这里。" className="workspace-artifacts-empty" /> : <div className="workspace-folder-list">
      {artifacts.map(artifact => <article key={artifact.id} className="workspace-folder-row workspace-artifact-row">
        <span className="workspace-result-icon">{artifact.mime_type?.startsWith("image/") ? <Image size={25} /> : <FileText size={25} />}</span>
        <div className="workspace-row-copy"><h3>{artifact.name}</h3><p>{formatTaskDate(artifact.updated_at || artifact.created_at)}{artifact.mime_type ? ` · ${artifact.mime_type}` : ""}</p></div>
        <Button variant="secondary" onClick={() => setSelected(artifact)}><ExternalLink size={16} />打开</Button>
      </article>)}
    </div>}
    {selected && <ArtifactPreviewDialog key={selected.id} artifact={selected} onClose={() => setSelected(null)} />}
  </section>;
}

function ArtifactPreviewDialog({ artifact, onClose }: { artifact: ArtifactRecord; onClose: () => void }) {
  const [preview, setPreview] = useState<ArtifactPreview | null>(null);
  const [imageUrl, setImageUrl] = useState("");
  const [error, setError] = useState("");
  const [downloading, setDownloading] = useState(false);
  useEffect(() => {
    let active = true; let objectUrl = "";
    void previewArtifact(artifact.id).then(result => { if (active) setPreview(result); }).catch(cause => { if (active) setError(cause instanceof Error ? cause.message : "成果预览读取失败"); });
    if (artifact.mime_type?.startsWith("image/")) void downloadArtifact(artifact.id).then(bytes => { if (active) { objectUrl = URL.createObjectURL(bytes); setImageUrl(objectUrl); } }).catch(cause => { if (active) setError(cause instanceof Error ? cause.message : "图片读取失败"); });
    return () => { active = false; if (objectUrl) URL.revokeObjectURL(objectUrl); };
  }, [artifact]);
  async function download() {
    setDownloading(true); setError("");
    try { const bytes = await downloadArtifact(artifact.id); const url = URL.createObjectURL(bytes); const link = document.createElement("a"); link.href = url; link.download = artifact.name; link.click(); window.setTimeout(() => URL.revokeObjectURL(url), 1000); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "成果下载失败"); }
    finally { setDownloading(false); }
  }
  return <Modal open title={artifact.name} size="xl" onClose={onClose} footer={<Button variant="secondary" loading={downloading} onClick={() => void download()}><Download size={16} />下载原文件</Button>}>
    <div className="workspace-artifact-preview">
      <div className="workspace-artifact-source"><span>{formatTaskDate(artifact.created_at)}</span>{preview?.provenance && <span>版本 {preview.provenance.version}</span>}{artifact.workspace_id && <Link to={`/workspaces/${encodeURIComponent(artifact.workspace_id)}`} onClick={onClose}>查看所属空间<ExternalLink size={13} /></Link>}</div>
      {error && <ErrorState title="成果暂时无法读取" description={error} />}
      {!preview && !error && <p role="status">正在读取成果…</p>}
      {imageUrl ? <img src={imageUrl} alt={artifact.name} /> : preview?.text != null ? /\.md$/i.test(artifact.name) || artifact.mime_type === "text/markdown" ? <article className="workspace-markdown"><ReactMarkdown remarkPlugins={[remarkGfm]}>{preview.text}</ReactMarkdown></article> : <pre className="workspace-text-preview">{preview.text}</pre> : preview && !artifact.mime_type?.startsWith("image/") ? <p className="workspace-tree-note">此格式暂不提供文本预览，可下载原文件后查看。</p> : null}
      {preview?.truncated && <p className="workspace-tree-note">当前展示部分内容，下载可取得完整文件。</p>}
      <details className="workspace-source-details"><summary>查看来源</summary><p>来源任务：{artifact.task_id}</p><p>来源执行：{artifact.task_execution_id}</p>{preview?.provenance && <pre>{JSON.stringify(preview.provenance.source, null, 2)}</pre>}</details>
    </div>
  </Modal>;
}
