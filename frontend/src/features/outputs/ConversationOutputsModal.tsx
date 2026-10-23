import { useEffect, useRef, useState } from "react";
import { Download, FileCode2, ImageIcon, RefreshCw } from "lucide-react";
import Modal from "../../components/ui/Modal";
import Button from "../../components/ui/Button";
import { getResource, getResourceContent, getResourcePreview, type Resource, type ResourcePreview } from "../../api/resources";
import { listCodeCandidates, listConversationOutputs, saveCodeOutput, type CodeCandidate, type ConversationOutput } from "../../api/conversationOutputs";
import "./conversationOutputs.css";

interface Props { conversationId: string; onClose: () => void; onRequestDownload: (id: string) => Promise<boolean>; canRequestDownload: boolean }
interface Preview { id: string; url: string; resource: Resource; preview: ResourcePreview }
const statusText = { ready: "已保存", awaiting_download: "待授权下载", download_failed: "下载失败" };
const candidateKey = (candidate: CodeCandidate) => `${candidate.message_id}:${candidate.block_index}`;

export default function ConversationOutputsModal({ conversationId, onClose, onRequestDownload, canRequestDownload }: Props) {
  const [outputs, setOutputs] = useState<ConversationOutput[]>([]);
  const [candidates, setCandidates] = useState<CodeCandidate[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [candidateId, setCandidateId] = useState("");
  const [previousId, setPreviousId] = useState("");
  const [revision, setRevision] = useState(0);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [previewError, setPreviewError] = useState("");
  const [preview, setPreview] = useState<Preview | null>(null);
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => {
    const controller = new AbortController();
    setLoading(true); setError("");
    void Promise.all([listConversationOutputs(conversationId, controller.signal), listCodeCandidates(conversationId, controller.signal)])
      .then(([nextOutputs, nextCandidates]) => {
        if (controller.signal.aborted) return;
        setOutputs(nextOutputs); setCandidates(nextCandidates);
        setSelectedId(current => nextOutputs.some(output => output.id === current) ? current : nextOutputs[nextOutputs.length - 1]?.id || "");
      }).catch(reason => { if (!controller.signal.aborted) setError(reason instanceof Error ? reason.message : "无法读取会话成果"); })
      .finally(() => { if (!controller.signal.aborted) setLoading(false); });
    return () => controller.abort();
  }, [conversationId, revision]);
  const selected = outputs.find(output => output.id === selectedId);
  const resourceId = selected?.status === "ready" ? selected.resource_id : null;
  useEffect(() => {
    const controller = new AbortController();
    let objectUrl: string | null = null;
    setPreview(null); setPreviewError("");
    if (resourceId) {
      void Promise.all([getResourceContent(resourceId, controller.signal), getResourcePreview(resourceId, controller.signal), getResource(resourceId)])
        .then(([blob, details, resource]) => {
          if (controller.signal.aborted) return;
          if (!resource) throw new Error("成果资源记录不存在");
          objectUrl = URL.createObjectURL(blob);
          setPreview({ id: resourceId, url: objectUrl, resource, preview: details });
        }).catch(reason => { if (!controller.signal.aborted) setPreviewError(reason instanceof Error ? reason.message : "成果预览读取失败"); });
    }
    return () => { controller.abort(); if (objectUrl) URL.revokeObjectURL(objectUrl); };
  }, [resourceId]);
  const available = candidates.filter(candidate => !outputs.some(output => output.source_message_id === candidate.message_id && output.block_index === candidate.block_index));
  const candidate = available.find(item => candidateKey(item) === candidateId);
  const currentPreview = preview?.id === resourceId ? preview : null;
  async function saveCode() {
    if (!candidate) return;
    setSaving(true); setError("");
    try {
      const output = await saveCodeOutput(conversationId, candidate, previousId);
      if (!alive.current) return;
      setSelectedId(output.id); setCandidateId(""); setPreviousId(""); setRevision(value => value + 1);
    } catch (reason) { if (alive.current) setError(reason instanceof Error ? reason.message : "代码成果保存失败"); }
    finally { if (alive.current) setSaving(false); }
  }
  async function continueDownload(id: string) {
    setSaving(true); setError("");
    try { if (await onRequestDownload(id)) onClose(); else if (alive.current) setError("当前对话暂不能发起操作，请先完成正在进行的执行或确认。"); }
    catch (reason) { if (alive.current) setError(reason instanceof Error ? reason.message : "未能发起下载"); }
    finally { if (alive.current) setSaving(false); }
  }
  return <Modal open onClose={onClose} title="当前对话成果" size="lg">
    <div className="conversation-outputs">
      <div className="conversation-outputs-intro"><p>查看已生成图片，或把消息中的代码保存为文件。</p><Button variant="ghost" size="sm" onClick={() => setRevision(value => value + 1)} disabled={loading || saving}><RefreshCw size={14}/> 刷新</Button></div>
      {error && <p className="conversation-outputs-error" role="alert">{error}</p>}
      {loading && <p role="status">正在读取成果…</p>}
      {!loading && !error && outputs.length === 0 && <p className="conversation-outputs-empty">当前对话还没有保存的成果。生成图片后会显示在这里；代码可在下方选择保存。</p>}
      {outputs.length > 0 && <div className="conversation-outputs-grid">
        <nav aria-label="成果列表" className="conversation-outputs-list">{outputs.map(output => <button type="button" key={output.id} className={selectedId === output.id ? "is-selected" : ""} aria-pressed={selectedId === output.id} onClick={() => setSelectedId(output.id)}>
          {output.kind === "image" ? <ImageIcon size={16}/> : <FileCode2 size={16}/>}<span><strong>{output.title}</strong><small>{statusText[output.status]}{output.kind === "code" ? ` · v${output.version}` : ""}</small></span>
        </button>)}</nav>
        <section className="conversation-output-detail" aria-label="成果详情">
          {selected && <><h4>{selected.title}</h4><p className="conversation-output-meta">{new Date(selected.created_at).toLocaleString()} · {selected.kind === "code" ? "来源：助手消息中的代码片段；保存不代表执行通过。" : "来源：本次对话的图像生成服务。"}</p>
            {selected.source_message_id && <p className="conversation-output-meta">消息 {selected.source_message_id.slice(0, 8)} · 第 {(selected.block_index ?? 0) + 1} 个代码块 · v{selected.version}</p>}
            {selected.previous_output_id && <button className="conversation-output-link" type="button" onClick={() => setSelectedId(selected.previous_output_id!)}>查看关联的前一版本</button>}
            {selected.status !== "ready" && <div className="conversation-output-pending"><p>{selected.error || "图片地址已收到，尚未保存到本地。下载会通过当前对话的执行授权。"}</p>{selected.can_download && <Button size="sm" variant="secondary" disabled={!canRequestDownload || saving} onClick={() => void continueDownload(selected.id)}>交给助手继续下载</Button>}</div>}
            {resourceId && !currentPreview && !previewError && <p role="status">正在读取本地成果…</p>}
            {previewError && <p className="conversation-outputs-error" role="alert">{previewError}</p>}
            {currentPreview && <>
              {currentPreview.preview.status !== "ready" && <p className="conversation-outputs-error">{currentPreview.preview.error || "此文件暂不能预览，可下载原文件。"}</p>}
              {selected.kind === "image" && currentPreview.preview.status === "ready" && <img src={currentPreview.url} alt={selected.title} className="conversation-output-image"/>}
              {selected.kind === "code" && currentPreview.preview.text != null && <pre className="conversation-output-code"><code>{currentPreview.preview.text}</code></pre>}
              {currentPreview.preview.truncated && <p className="conversation-output-meta">当前为部分预览，下载可取得完整文件。</p>}
              <a className="conversation-output-download" href={currentPreview.url} download={currentPreview.resource.name}><Download size={15}/> 下载原文件 · {(currentPreview.resource.size / 1024).toFixed(1)} KB</a>
            </>}
          </>}
        </section>
      </div>}
      {available.length > 0 && <section className="conversation-output-save"><h4>保存消息中的代码</h4><label>代码片段<select value={candidateId} onChange={event => setCandidateId(event.target.value)} disabled={saving}><option value="">选择一段已完成的代码</option>{available.map(item => <option key={candidateKey(item)} value={candidateKey(item)}>{item.language} · {new Date(item.created_at).toLocaleTimeString()} · 第 {item.block_index + 1} 段</option>)}</select></label>
        {candidate && <><pre className="conversation-output-code"><code>{candidate.preview}</code></pre><label>版本关系<select value={previousId} onChange={event => setPreviousId(event.target.value)} disabled={saving}><option value="">作为独立成果保存</option>{outputs.filter(output => output.kind === "code" && output.status === "ready").map(output => <option key={output.id} value={output.id}>接续 {output.title} · v{output.version} · {output.id.slice(0, 6)}</option>)}</select></label><Button size="sm" disabled={saving} onClick={() => void saveCode()}>{saving ? "正在保存…" : "保存代码文件"}</Button></>}
      </section>}
    </div>
  </Modal>;
}
