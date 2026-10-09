import { useEffect, useState } from "react";
import { getResourceContent, getResourcePreview, type ResourcePreview } from "../../api/resources";
import type { Attachment } from "./attachmentQueue";

const LABELS = { queued: "等待上传", uploading: "上传中", parsing: "获取解析结果", ready: "已就绪", failed: "失败", removed: "已移除" };

export function ControlledResourcePreview({ resourceId }: { resourceId: string }) {
  const [preview, setPreview] = useState<ResourcePreview | null>(null);
  const [error, setError] = useState(""); const [imageUrl, setImageUrl] = useState("");
  useEffect(() => {
    const controller = new AbortController(); let url = "";
    setPreview(null); setError(""); setImageUrl("");
    void getResourcePreview(resourceId, controller.signal).then(async result => {
      if (controller.signal.aborted) return;
      setPreview(result);
      if (result.status === "ready" && result.kind === "image") {
        const bytes = await getResourceContent(resourceId, controller.signal);
        if (controller.signal.aborted) return;
        url = URL.createObjectURL(bytes); setImageUrl(url);
      }
    }).catch(error => { if (!controller.signal.aborted) setError(error instanceof Error ? error.message : "无法预览"); });
    return () => { controller.abort(); if (url) URL.revokeObjectURL(url); };
  }, [resourceId]);
  if (error) return <p role="alert">{error}</p>;
  if (!preview) return <p role="status">加载预览…</p>;
  if (preview.status !== "ready") return <p role="status">{preview.error || "此资源暂不能解析"}</p>;
  return <div className="mt-2 max-h-64 overflow-auto rounded border border-[var(--border)] p-2 text-xs">
    {imageUrl ? <img src={imageUrl} alt="用户上传图片的受控预览" className="max-h-56 max-w-full object-contain" /> : null}
    {preview.text !== null ? <pre className="whitespace-pre-wrap break-words">{preview.text}</pre> : null}
    {preview.truncated ? <p>预览已截断；原文件仍保留。</p> : null}
  </div>;
}

export default function ResourceAttachments({ items, onRemove }: { items: Attachment[]; onRemove: (id: string) => void }) {
  return <ul aria-label="输入附件" className="space-y-1 px-2 py-2 text-xs">
    {items.map(item => <li key={item.id} className="rounded border border-[var(--border)] p-2">
      <div className="flex items-center gap-2"><span className="min-w-0 flex-1 truncate">{item.name}</span><span role="status">{LABELS[item.state]}</span>
        {item.state !== "removed" ? <button type="button" aria-label={`移除 ${item.name}`} onClick={() => onRemove(item.id)}>移除</button> : null}
      </div>
      {item.error && item.state !== "removed" ? <p role="alert" className="mt-1 text-[var(--danger)]">{item.error}</p> : null}
      {item.state === "ready" && item.resourceId ? <ResourcePreviewDisclosure resourceId={item.resourceId} /> : null}
    </li>)}
  </ul>;
}

export function ResourcePreviewDisclosure({ resourceId }: { resourceId: string }) {
  const [open, setOpen] = useState(false);
  return <details onToggle={event => setOpen(event.currentTarget.open)}><summary className="mt-1 cursor-pointer">预览输入文件</summary>{open ? <ControlledResourcePreview resourceId={resourceId} /> : null}</details>;
}
