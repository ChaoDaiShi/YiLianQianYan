import { useEffect, useState } from "react";
import { getResourceContent, getResourcePreview, type ResourcePreview } from "../../api/resources";
import { MAX_ATTACHMENTS, type Attachment } from "./attachmentQueue";
import { ResourceCard } from "./ResourceCard";

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

export default function ResourceAttachments({ items, onRemove, onRetry, disabled }: {
  items: Attachment[]; onRemove: (id: string) => void; onRetry: (id: string) => void; disabled: boolean;
}) {
  return <section aria-label="待发送附件">
    <p className="composer-attachment-count">附件 {items.length}/{MAX_ATTACHMENTS} · 每个最多 25 MiB</p>
    <ul aria-label="输入附件" className="resource-card-grid">
      {items.map(item => <li key={item.id}>
        <ResourceCard file={{ id: item.state === "ready" ? item.resourceId || "" : "", name: item.name, mime_type: item.mimeType, size: item.size }}
          onRemove={() => onRemove(item.id)} onRetry={item.retryable ? () => onRetry(item.id) : undefined} actionsDisabled={disabled}
          status={LABELS[item.state]} error={item.error} upload={item.state === "uploading" ? { loaded: item.loaded, total: item.size } : undefined} />
      </li>)}
    </ul>
  </section>;
}

export function ResourcePreviewDisclosure({ resourceId }: { resourceId: string }) {
  const [open, setOpen] = useState(false);
  return <details onToggle={event => setOpen(event.currentTarget.open)}><summary className="mt-1 cursor-pointer">预览输入文件</summary>{open ? <ControlledResourcePreview resourceId={resourceId} /> : null}</details>;
}
