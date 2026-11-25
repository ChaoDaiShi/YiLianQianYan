import { useEffect, useState } from "react";
import { getResourceContent } from "../../api/resources";

export interface ReplyImage {
  id: string;
  resourceId: string;
  title: string;
}

export type OpenReplyImage = (title: string, blob: Blob) => void;

function ReplyImagePreview({ image, onOpen }: { image: ReplyImage; onOpen: OpenReplyImage }) {
  const [preview, setPreview] = useState<{ url: string; blob: Blob } | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    let objectUrl = "";
    void getResourceContent(image.resourceId, controller.signal).then(blob => {
      if (controller.signal.aborted) return;
      objectUrl = URL.createObjectURL(blob);
      setPreview({ url: objectUrl, blob });
    }).catch(() => {
      if (!controller.signal.aborted) setFailed(true);
    });
    return () => {
      controller.abort();
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [image.resourceId]);

  if (failed) return <p className="text-xs text-[var(--text-muted)]">生成的图片暂时无法预览</p>;
  if (!preview) return <p role="status" className="text-xs text-[var(--text-muted)]">正在加载生成的图片…</p>;

  return (
    <button
      type="button"
      aria-label={`放大生成的图片 ${image.title}`}
      className="max-w-full cursor-zoom-in rounded-lg border border-[var(--border-soft)] bg-[var(--surface-muted)]"
      onClick={() => onOpen(image.title, preview.blob)}
    >
      <img
        src={preview.url}
        alt={image.title}
        loading="lazy"
        className="block max-h-60 max-w-full rounded-lg object-contain"
        onError={() => setFailed(true)}
      />
    </button>
  );
}

export default function ReplyImagePreviews({ images, onOpen }: { images?: readonly ReplyImage[]; onOpen?: OpenReplyImage }) {
  if (!images?.length || !onOpen) return null;
  return (
    <div className="mt-3 flex flex-wrap items-start gap-2" role="group" aria-label="生成的图片">
      {images.map(image => <ReplyImagePreview key={`${image.id}:${image.resourceId}`} image={image} onOpen={onOpen} />)}
    </div>
  );
}
