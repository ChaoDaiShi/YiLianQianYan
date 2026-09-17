export type AttachmentState = "queued" | "uploading" | "parsing" | "ready" | "failed" | "removed";
export interface Attachment { id: string; name: string; state: AttachmentState; resourceId?: string; error?: string }
export interface AttachmentQueueOptions {
  upload: (file: File, signal: AbortSignal) => Promise<{ id: string }>;
  preview: (id: string, signal: AbortSignal) => Promise<{ status: string; error?: string }>;
  changed?: () => void;
}
export const MAX_ATTACHMENTS = 8;
export const MAX_ATTACHMENT_BYTES = 25 * 1024 * 1024;
const EXTENSIONS = new Set(["txt", "md", "markdown", "csv", "tsv", "rs", "ts", "tsx", "js", "jsx", "json", "py", "css", "html", "yaml", "yml", "toml", "xml", "sql", "log", "png", "jpg", "jpeg", "pdf", "docx", "xlsx"]);

export function validateAttachment(file: Pick<File, "name" | "size" | "type">): string | null {
  if (!file.size || file.size > MAX_ATTACHMENT_BYTES) return "文件须为 1 字节至 25 MiB";
  const extension = file.name.split(".").pop()?.toLowerCase() ?? "";
  if (!EXTENSIONS.has(extension)) return "暂不支持此文件类型";
  if (file.name.length > 255 || /[\u0000-\u001f]/.test(file.name)) return "文件名无效";
  return null;
}

export class AttachmentQueue {
  private items: Attachment[] = [];
  private files = new Map<string, File>();
  private controllers = new Map<string, AbortController>();
  private processing: Promise<void> | null = null;
  private disposed = false;
  constructor(private options: AttachmentQueueOptions) {}
  add(files: File[]): void {
    if (this.disposed) return;
    if (this.items.filter(item => item.state !== "removed").length + files.length > MAX_ATTACHMENTS) throw new Error("一次最多添加 8 个文件");
    // Removed entries are a local history, bounded to the current batch.
    this.items = this.items.filter(item => item.state !== "removed");
    for (const file of files) {
      const error = validateAttachment(file);
      const item: Attachment = { id: crypto.randomUUID(), name: file.name, state: error ? "failed" : "queued", ...(error ? { error } : {}) };
      this.items.push(item);
      if (!error) this.files.set(item.id, file);
    }
    this.options.changed?.();
  }
  remove(id: string): void {
    const item = this.items.find(item => item.id === id);
    if (!item) return;
    this.controllers.get(id)?.abort(); this.files.delete(id);
    item.state = "removed"; item.resourceId = undefined;
    this.options.changed?.();
  }
  snapshot(): Attachment[] { return this.items.map(item => ({ ...item })); }
  readyIds(): string[] { return this.items.filter(item => item.state === "ready" && item.resourceId).map(item => item.resourceId!); }
  process(): Promise<void> {
    if (this.processing) return this.processing;
    this.processing = this.run().finally(() => { this.processing = null; });
    return this.processing;
  }
  private async run(): Promise<void> {
    while (!this.disposed) {
      const item = this.items.find(item => item.state === "queued");
      if (!item) break;
      const file = this.files.get(item.id); if (!file) continue;
      const controller = new AbortController(); this.controllers.set(item.id, controller);
      const current = () => !this.disposed && !controller.signal.aborted && item.state !== "removed";
      try {
        item.state = "uploading"; this.options.changed?.();
        const resource = await this.options.upload(file, controller.signal);
        if (!current()) continue;
        item.resourceId = resource.id; item.state = "parsing"; this.options.changed?.();
        const preview = await this.options.preview(resource.id, controller.signal);
        if (!current()) continue;
        item.state = preview.status === "ready" ? "ready" : "failed";
        item.error = item.state === "failed" ? preview.error || "无法解析此文件，请移除后重试" : undefined;
      } catch (error) {
        if (current()) { item.state = "failed"; item.error = error instanceof Error ? error.message : "资源导入失败"; }
      } finally {
        this.controllers.delete(item.id); this.files.delete(item.id);
        if (current()) this.options.changed?.();
      }
    }
  }
  dispose(): void {
    this.disposed = true; this.controllers.forEach(controller => controller.abort()); this.controllers.clear(); this.files.clear();
  }
}
