import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Download, Minus, Plus } from "lucide-react";
import Modal from "../../components/ui/Modal";
import { getResourceContent, getResourcePreview, type PreviewBlock, type ResourcePreview } from "../../api/resources";
import { getWorkspaceFileContent, getWorkspaceFilePreview } from "../../api/workspaceFiles";
import "./resourceCards.css";

export type PreviewSource = { kind: "resource"; id: string } | { kind: "workspace"; workspaceId: string; path: string };

function Table({ rows }: { rows: string[][] }) {
  return <div className="resource-preview-table-scroll"><table className="resource-preview-table"><tbody>{rows.map((row, i) => <tr key={i}>{row.map((cell, j) => <td key={j}>{cell}</td>)}</tr>)}</tbody></table></div>;
}

const kindLabels: Record<string, string> = {
  docx: "Word 文档", rtf: "RTF 文档", xlsx: "Excel 工作簿", xls: "Excel 工作簿",
  pptx: "PowerPoint 演示文稿", pdf: "PDF 文档", zip: "ZIP 压缩包",
  image: "图片", audio: "音频", video: "视频", text: "文本文件",
};

function Blocks({ blocks }: { blocks: PreviewBlock[] }) {
  return <>{blocks.map((block, index) => {
    if (block.kind === "table") return <Table key={index} rows={block.rows ?? []} />;
    if (block.kind === "image") return block.src ? <img key={index} className="resource-preview-embedded-image" src={block.src} alt={block.alt ?? "文档中的图片"} /> : null;
    const content = block.runs?.length ? block.runs.map((run, i) => <span key={i} style={{ fontWeight: run.bold ? "bold" : undefined, fontStyle: run.italic ? "italic" : undefined, textDecoration: run.underline ? "underline" : undefined }}>{run.text}</span>) : block.text;
    return block.kind === "heading" ? <div key={index} role="heading" aria-level={Math.min(6, Math.max(1, block.level ?? 1))} className="resource-preview-heading" data-level={block.level}>{content}</div> : <p key={index}>{content || "\u00a0"}</p>;
  })}</>;
}

function SearchableText({ text, query, selected }: { text: string; query: string; selected: number }) {
  const target = useRef<HTMLElement>(null);
  const parts = useMemo(() => {
    if (!query) return [{ text, match: false }];
    const result: { text: string; match: boolean }[] = [];
    const source = text.toLocaleLowerCase(), needle = query.toLocaleLowerCase();
    let start = 0, at = source.indexOf(needle);
    while (at !== -1) { result.push({ text: text.slice(start, at), match: false }, { text: text.slice(at, at + query.length), match: true }); start = at + query.length; at = source.indexOf(needle, start); }
    result.push({ text: text.slice(start), match: false });
    return result;
  }, [text, query]);
  useEffect(() => { target.current?.scrollIntoView({ block: "nearest" }); }, [selected, query]);
  let match = -1;
  return <pre className="resource-preview-text">{parts.map((part, i) => part.match ? <mark key={i} ref={++match === selected ? target : undefined} data-selected={match === selected}>{part.text}</mark> : part.text)}</pre>;
}

interface PreviewProps { file: { name: string; size?: number }; source: PreviewSource; onMetadataChange?: (metadata: string) => void }

export function ResourcePreviewPane({ file, source, onMetadataChange }: PreviewProps) {
  const [preview, setPreview] = useState<ResourcePreview | null>(null);
  const [url, setUrl] = useState("");
  const [fullText, setFullText] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [downloading, setDownloading] = useState(false);
  const [zoom, setZoom] = useState(100);
  const [sheet, setSheet] = useState(0);
  const [slide, setSlide] = useState(0);
  const [query, setQuery] = useState("");
  const [match, setMatch] = useState(0);
  const contentRef = useRef<HTMLDivElement>(null);
  const [documentMatches, setDocumentMatches] = useState<Range[]>([]);
  const resourceId = source.kind === "resource" ? source.id : "";
  const workspaceId = source.kind === "workspace" ? source.workspaceId : "";
  const workspacePath = source.kind === "workspace" ? source.path : "";

  useEffect(() => {
    const abort = new AbortController(); let objectUrl = "";
    void (async () => {
      const result = source.kind === "resource" ? await getResourcePreview(resourceId, abort.signal) : await getWorkspaceFilePreview(workspaceId, workspacePath, abort.signal);
      if (abort.signal.aborted) return;
      setPreview(result);
      if (result.status !== "ready") return;
      if (["image", "pdf", "audio", "video", "text"].includes(result.kind)) {
        const bytes = source.kind === "resource" ? await getResourceContent(resourceId, abort.signal) : await getWorkspaceFileContent(workspaceId, workspacePath, abort.signal);
        if (abort.signal.aborted) return;
        if (result.kind === "text") setFullText(await bytes.text());
        else { objectUrl = URL.createObjectURL(bytes); setUrl(objectUrl); }
      }
    })().catch(cause => { if (!abort.signal.aborted) setError(cause instanceof Error ? cause.message : "无法读取预览"); });
    return () => { abort.abort(); if (objectUrl) URL.revokeObjectURL(objectUrl); };
  }, [source.kind, resourceId, workspaceId, workspacePath]);

  async function download() {
    setDownloading(true);
    try { const bytes = source.kind === "resource" ? await getResourceContent(resourceId) : await getWorkspaceFileContent(workspaceId, workspacePath); const href = URL.createObjectURL(bytes); const link = document.createElement("a"); link.href = href; link.download = file.name; link.click(); window.setTimeout(() => URL.revokeObjectURL(href), 1000); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "下载失败"); }
    finally { setDownloading(false); }
  }
  const count = useMemo(() => {
    if (!query || fullText === null) return 0;
    const source = fullText.toLocaleLowerCase(), needle = query.toLocaleLowerCase(); let n = 0, position = 0;
    while ((position = source.indexOf(needle, position)) !== -1) { n++; position += needle.length; }
    return n;
  }, [query, fullText]);
  const presentation = preview?.presentation;
  const searchableDocument = preview?.status === "ready" && ["docx", "rtf", "xlsx", "xls", "pptx", "zip"].includes(preview.kind);
  useEffect(() => {
    if (!searchableDocument || !query || !contentRef.current) { setDocumentMatches([]); return; }
    const content = contentRef.current.querySelector(".resource-preview-paper, .resource-preview-sheet .resource-preview-table-scroll, .resource-preview-entries");
    if (!content) { setDocumentMatches([]); return; }
    const ranges: Range[] = [];
    const pattern = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "giu");
    content.querySelectorAll("p, .resource-preview-heading, td").forEach(element => {
      const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
      const nodes: { node: Node; start: number; end: number }[] = [];
      let text = "";
      while (walker.nextNode()) {
        const node = walker.currentNode; const value = node.textContent ?? "";
        nodes.push({ node, start: text.length, end: text.length + value.length }); text += value;
      }
      pattern.lastIndex = 0;
      let found: RegExpExecArray | null;
      while ((found = pattern.exec(text)) !== null) {
        const at = found.index; const end = at + found[0].length;
        const first = nodes.find(item => item.start <= at && item.end > at);
        const last = nodes.find(item => item.start < end && item.end >= end);
        if (first && last) {
          const range = document.createRange(); range.setStart(first.node, at - first.start); range.setEnd(last.node, end - last.start); ranges.push(range);
        }
      }
    });
    setDocumentMatches(ranges);
  }, [query, searchableDocument, presentation, sheet, slide]);
  useEffect(() => {
    const range = documentMatches[match];
    if (!range) return;
    const selection = window.getSelection(); selection?.removeAllRanges(); selection?.addRange(range);
    range.startContainer.parentElement?.scrollIntoView({ block: "nearest" });
    return () => { selection?.removeAllRanges(); };
  }, [documentMatches, match]);
  const searchCount = searchableDocument ? documentMatches.length : count;
  const canZoom = preview?.status === "ready" && ["image", "docx", "rtf", "xlsx", "xls", "pptx", "text"].includes(preview.kind);
  const canSearch = searchableDocument || (preview?.kind === "text" && fullText !== null);
  const metadata = `${preview ? (kindLabels[preview.kind] ?? preview.kind.toUpperCase()) : "文件"}${typeof file.size === "number" ? ` · ${file.size < 1024 * 1024 ? `${Math.ceil(file.size / 1024)} KB` : `${(file.size / 1024 / 1024).toFixed(1)} MB`}` : ""}`;
  useEffect(() => { onMetadataChange?.(metadata); }, [metadata, onMetadataChange]);
  return <section className="resource-preview-workspace" aria-label={`预览内容 ${file.name}`}>
      {(!onMetadataChange || canZoom || canSearch) && <div className="resource-preview-toolbar">
        {!onMetadataChange && <span className="resource-preview-meta">{metadata}</span>}
        {canZoom && <div className="resource-preview-controls"><button type="button" aria-label="缩小" disabled={zoom <= 50} onClick={() => setZoom(value => value - 10)}><Minus size={15}/></button><span>{zoom}%</span><button type="button" aria-label="放大" disabled={zoom >= 200} onClick={() => setZoom(value => value + 10)}><Plus size={15}/></button></div>}
        {canSearch && <div className="resource-preview-search"><input aria-label="搜索文件内容" placeholder="搜索当前内容" value={query} onChange={event => { setQuery(event.target.value); setMatch(0); }}/><span>{searchCount ? `${Math.min(match + 1, searchCount)}/${searchCount}` : "0/0"}</span><button type="button" aria-label="上一处匹配" disabled={!searchCount} onClick={() => setMatch((match + searchCount - 1) % searchCount)}><ChevronLeft size={15}/></button><button type="button" aria-label="下一处匹配" disabled={!searchCount} onClick={() => setMatch((match + 1) % searchCount)}><ChevronRight size={15}/></button></div>}
      </div>}
      {error && <p role="alert" className="resource-card-error">{error}</p>}
      {!preview && !error && <p role="status">正在读取文件内容…</p>}
      {preview && preview.status !== "ready" && <p role="status">{preview.error || "此文件暂不支持内容预览，可下载原文件。"}</p>}
      {preview?.status === "ready" && <div className="resource-preview-body" ref={contentRef}>
        {preview.kind === "pdf" && (url ? <iframe title={`${file.name} 的 PDF 预览`} src={url} className="resource-preview-pdf"/> : !error && <p role="status">正在加载 PDF…</p>)}
        {preview.kind === "image" && (url ? <div className="resource-preview-image"><img src={url} alt={file.name} style={{ width: `${zoom}%` }} onError={() => setError("图片无法显示，请下载原文件查看。")}/></div> : !error && <p role="status">正在加载图片…</p>)}
        {preview.kind === "audio" && (url ? <audio controls src={url} onError={() => setError("当前浏览器无法播放此音频，请下载原文件。")}>浏览器不支持此音频播放。</audio> : !error && <p role="status">正在加载音频…</p>)}
        {preview.kind === "video" && (url ? <video controls src={url} onError={() => setError("当前浏览器无法播放此视频，请下载原文件。")}>浏览器不支持此视频播放。</video> : !error && <p role="status">正在加载视频…</p>)}
        {preview.kind === "text" && (fullText !== null ? <div className="resource-preview-paper" style={{ fontSize: `${zoom}%` }}><SearchableText text={fullText} query={query} selected={match}/></div> : !error && <p role="status">正在加载正文…</p>)}
        {(preview.kind === "docx" || preview.kind === "rtf") && presentation && <div className="resource-preview-paper" style={{ fontSize: `${zoom}%` }}><Blocks blocks={presentation.blocks}/></div>}
        {(preview.kind === "xlsx" || preview.kind === "xls") && presentation && <div className="resource-preview-sheet" style={{ fontSize: `${zoom}%` }}><div className="resource-preview-tabs" role="tablist" aria-label="工作表">{presentation.sheets.map((item, index) => <button role="tab" aria-selected={sheet === index} type="button" key={index} onClick={() => { setSheet(index); setMatch(0); }}>{item.name}</button>)}</div>{presentation.sheets[sheet] && <Table rows={presentation.sheets[sheet].rows}/>}</div>}
        {preview.kind === "pptx" && presentation && <div className="resource-preview-slides"><div className="resource-preview-slide-nav"><button type="button" disabled={slide === 0} onClick={() => { setSlide(slide - 1); setMatch(0); }}><ChevronLeft size={15}/>上一页</button><span>{presentation.slides.length ? slide + 1 : 0} / {presentation.slides.length}</span><button type="button" disabled={slide >= presentation.slides.length - 1} onClick={() => { setSlide(slide + 1); setMatch(0); }}>下一页<ChevronRight size={15}/></button></div><div className="resource-preview-slide-layout"><div className="resource-preview-thumbnails" aria-label="幻灯片列表">{presentation.slides.map((item, index) => <button type="button" key={index} aria-current={index === slide ? "page" : undefined} onClick={() => { setSlide(index); setMatch(0); }}><span>第 {item.number} 页</span><span className="resource-preview-thumbnail-content">{item.blocks.filter(block => block.kind === "heading" || block.kind === "paragraph").slice(0, 3).map(block => block.text || block.runs?.map(run => run.text).join("")).join(" · ")}</span></button>)}</div>{presentation.slides[slide] && <div className="resource-preview-paper resource-preview-slide" style={{ fontSize: `${zoom}%` }}><Blocks blocks={presentation.slides[slide].blocks}/></div>}</div><p className="resource-preview-note">显示提取的内容和图片，幻灯片版式可能与原文件不同。</p></div>}
        {preview.kind === "zip" && presentation && <div className="resource-preview-entries"><table className="resource-preview-table"><thead><tr><th>名称</th><th>大小</th></tr></thead><tbody>{presentation.entries.map((entry, index) => <tr key={index}><td>{entry.directory ? "文件夹 · " : ""}{entry.name}</td><td>{entry.directory ? "—" : `${entry.size} B`}</td></tr>)}</tbody></table></div>}
      </div>}
    <footer className="resource-preview-footer"><button type="button" className="resource-download-button" disabled={downloading} onClick={() => void download()}><Download size={15}/>{downloading ? "正在读取原文件…" : "下载原文件"}</button></footer>
  </section>;
}

export default function ResourcePreviewDialog({ file, source, onClose }: PreviewProps & { onClose: () => void }) {
  const identity = source.kind === "resource" ? `resource:${source.id}` : `workspace:${source.workspaceId}:${source.path}`;
  return <Modal open onClose={onClose} title={file.name} size="xl" className="resource-preview-dialog">
    <ResourcePreviewPane key={identity} file={file} source={source}/>
  </Modal>;
}
