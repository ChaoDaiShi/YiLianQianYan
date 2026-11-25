import { useState } from "react";
import { managementRequest } from "../../api/productManagement";
import { Button, Input, Modal, Select, Textarea } from "../../components/ui";
import type { ManagedImport } from "./ManagedImports";

type Format = "zip" | "plugin_source" | "github" | "markdown" | "directory";
const maxPackageBytes = 2 * 1024 * 1024;
const maxFileBytes = 512 * 1024;
const maxEntries = 64;

interface ImportPreview {
  preview_id: string;
  expected_revision: number | null;
  package: {
    id: string;
    name: string;
    version: string;
    kind: string;
    permissions: string[];
    content_hash: string;
    files: Record<string, string>;
    file_encodings: Record<string, "base64">;
  };
  added: string[];
  changed: string[];
  removed: string[];
  previous_permissions: string[];
  unchanged: boolean;
}

const errors: Record<string, string> = {
  invalid_package_id: "插件 ID 只能使用小写字母、数字、点、下划线和连字符。",
  invalid_package_metadata: "请检查名称、版本和文件数量。版本只能使用字母、数字、点、加号和连字符。",
  invalid_zip: "无法读取 ZIP，请检查文件是否完整。",
  invalid_zip_directory: "无法读取 ZIP，请检查文件是否完整。",
  zip_input_limit: "ZIP 文件大小不能超过 2 MB。",
  zip_expansion_limit: "ZIP 解包后的文件大小超出限制。",
  invalid_plugin_manifest: "plugin.json 无法通过校验，请检查 JSON 格式、必填字段和贡献声明。",
  plugin_manifest_missing: "未找到 plugin.json，请选择插件目录或插件 ZIP。",
  plugin_archive_required: "所选 ZIP 是 Skill 包。请从「导入能力」导入 Skill。",
  missing_skill_markdown: "包里没有找到插件声明或可导入的 Skill。",
  missing_plugin_contribution: "声明引用的文件缺失。",
  plugin_contribution_requires_utf8: "声明引用的文件需要使用 UTF-8 文本。",
  invalid_plugin_workflow: "工作流声明无效。",
  invalid_plugin_agent: "智能体声明无效。",
  invalid_package_resource: "无法读取包内文件。",
  import_kind_conflict: "此 ID 已用于另一种能力，请为插件使用独立 ID。",
  stale_import_preview: "安装记录已变化，请返回重新检查。",
  preview_missing_or_expired: "预览已失效，请返回重新检查。",
  too_many_pending_previews: "有多个未确认的预览，请关闭其他预览或稍后重试。",
  unsafe_package_path: "包内文件名或路径不受支持。",
  github_plugin_zip_required: "请填写仓库内的 ZIP 文件路径。",
  immutable_public_github_source_required: "请填写公开 GitHub 仓库地址和完整的 40 位提交哈希。",
};

export function formatImportError(cause: unknown) {
  const message = cause instanceof Error ? cause.message : String(cause);
  const separator = message.indexOf(": ");
  const code = separator < 0 ? message : message.slice(0, separator);
  return errors[code] ? errors[code] + (separator < 0 ? "" : " " + message.slice(separator + 2)) : message;
}

async function base64File(file: File) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 8192) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 8192));
  }
  return btoa(binary);
}

async function pluginSource(files: File[]) {
  const entries = files.map(file => ({
    file,
    path: file.webkitRelativePath.split("/").slice(1).join("/"),
  }));
  if (!entries.some(entry => entry.path === "plugin.json")) {
    throw new Error("请直接选择包含 plugin.json 的插件目录。");
  }
  if (entries.length > maxEntries || entries.some(entry => entry.file.size > maxFileBytes)
    || entries.reduce((total, entry) => total + entry.file.size, 0) > maxPackageBytes) {
    throw new Error("插件目录最多 64 个文件，单个文件不超过 512 KB，总计不超过 2 MB。");
  }
  return Object.fromEntries(await Promise.all(entries.map(async ({ path, file }) => [path, await base64File(file)])));
}

const pluginSources = [
  { value: "zip", label: "本地 ZIP" },
  { value: "plugin_source", label: "本地插件目录" },
  { value: "github", label: "公开 GitHub ZIP" },
];
const packageSources = [
  { value: "directory", label: "本地 Skill 目录" },
  { value: "markdown", label: "Markdown Skill" },
  { value: "zip", label: "ZIP 声明包" },
  { value: "github", label: "公开 GitHub" },
];

export default function PackageImportDialog({ pluginOnly, initialDirectory, initialName, onClose, onImported }: {
  pluginOnly: boolean;
  initialDirectory?: string;
  initialName?: string;
  onClose: () => void;
  onImported: (record: ManagedImport) => void;
}) {
  const [format, setFormat] = useState<Format>(pluginOnly ? "zip" : initialDirectory ? "directory" : "markdown");
  const [id, setId] = useState("");
  const [name, setName] = useState(initialName ?? "");
  const [version, setVersion] = useState("1.0.0");
  const [content, setContent] = useState("");
  const [directory, setDirectory] = useState(initialDirectory ?? "");
  const [file, setFile] = useState<File | null>(null);
  const [folder, setFolder] = useState<File[]>([]);
  const [repository, setRepository] = useState("");
  const [commit, setCommit] = useState("");
  const [path, setPath] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const needsMetadata = format === "directory" || format === "markdown" || (format === "github" && !pluginOnly && path.endsWith(".md"));
  const ready = (!needsMetadata || Boolean(id.trim() && name.trim() && version.trim())) &&
    (format === "zip" ? Boolean(file) : format === "plugin_source" ? folder.length > 0
      : format === "directory" ? Boolean(directory.trim()) : format === "markdown" ? Boolean(content.trim())
      : Boolean(repository.trim() && commit.trim() && path.trim()));

  async function run(action: () => Promise<void>) {
    setBusy(true);
    setError("");
    try {
      await action();
    } catch (cause) {
      setError(formatImportError(cause));
    } finally {
      setBusy(false);
    }
  }

  async function inspect() {
    await run(async () => {
      let request: Record<string, unknown>;
      if (format === "plugin_source") request = { format, files: await pluginSource(folder) };
      else if (format === "zip" && file) {
        if (file.size > maxPackageBytes) throw new Error("ZIP 文件大小不能超过 2 MB。");
        request = { format, data_base64: await base64File(file), plugin_only: pluginOnly };
      } else if (format === "github") {
        request = { format, id, name, version, repository: repository.trim(), commit: commit.trim(), path: path.trim(), plugin_only: pluginOnly };
      } else if (format === "directory") request = { format, id, name, version, directory: directory.trim() };
      else request = { format, id, name, version, content };
      setPreview(await managementRequest<ImportPreview>("POST", "/api/capabilities/imports/inspect", request));
    });
  }

  function close() {
    if (busy) return;
    if (preview) void managementRequest("DELETE", "/api/capabilities/imports/previews/" + encodeURIComponent(preview.preview_id)).catch(() => {});
    onClose();
  }

  function back() {
    if (!preview) return;
    void run(async () => {
      await managementRequest("DELETE", "/api/capabilities/imports/previews/" + encodeURIComponent(preview.preview_id));
      setPreview(null);
    });
  }

  async function confirm() {
    if (!preview) return;
    await run(async () => {
      const record = await managementRequest<ManagedImport>("POST", "/api/capabilities/imports/confirm", { preview_id: preview.preview_id });
      onImported(record);
    });
  }

  const addedPermissions = preview?.package.permissions.filter(permission => !preview.previous_permissions.includes(permission)) ?? [];
  const removedPermissions = preview?.previous_permissions.filter(permission => !preview.package.permissions.includes(permission)) ?? [];
  const manifest = preview?.package.kind === "plugin" ? JSON.parse(preview.package.files["plugin.json"]) as {
    contributes?: { features?: Array<{ title: string }> };
  } : null;
  const featureNames = manifest?.contributes?.features?.map(feature => feature.title) ?? [];
  return (
    <Modal open onClose={close} title={preview ? "检查安装预览" : pluginOnly ? "导入插件" : "导入 Skill 或插件"} size="lg" footer={
      <div className="settings-dialog-footer">
        {error && <p className="settings-save-error settings-dialog-error" role="alert">{error}</p>}
        <div className="settings-dialog-actions">
          <Button variant="secondary" disabled={busy} onClick={close}>关闭</Button>
          {preview ? <>
            <Button variant="secondary" disabled={busy} onClick={back}>返回修改</Button>
            <Button disabled={busy} onClick={() => void confirm()}>{preview.unchanged ? "查看已安装版本" : "确认安装或更新"}</Button>
          </> : <Button disabled={busy || !ready} onClick={() => void inspect()}>{busy ? "正在检查…" : "检查并生成预览"}</Button>}
        </div>
      </div>
    }>
      {preview ? (
        <div className="space-y-3 break-words" aria-label="导入变更预览">
          <h3 className="font-semibold">{preview.package.name} · v{preview.package.version}</h3>
          <p className="text-sm">新增 {preview.added.length} · 修改 {preview.changed.length} · 移除 {preview.removed.length}</p>
          {featureNames.length > 0 && <p className="text-sm">功能入口 {featureNames.join("、")}</p>}
          <p className="text-sm">声明权限 {preview.package.permissions.join("、") || "无"}</p>
          {preview.expected_revision !== null && (addedPermissions.length > 0 || removedPermissions.length > 0) && <p className="text-sm">
            新增权限 {addedPermissions.join("、") || "无"} · 移除权限 {removedPermissions.join("、") || "无"}
          </p>}
          <details className="u2-disclosure">
            <summary>文件变更与声明详情</summary>
            <div className="space-y-2 pt-2">
              {preview.removed.length > 0 && <p className="text-sm">移除文件 {preview.removed.join("、")}</p>}
              {Object.entries(preview.package.files).map(([path, text]) => (
                <details key={path}>
                  <summary>{path}{preview.added.includes(path) ? " · 新增" : preview.changed.includes(path) ? " · 修改" : ""}</summary>
                  <pre className="max-h-52 overflow-auto whitespace-pre-wrap break-words p-2 text-xs">
                    {preview.package.file_encodings[path] === "base64" ? "二进制文件" : text}
                  </pre>
                </details>
              ))}
              <p className="break-all text-xs">SHA-256 {preview.package.content_hash}</p>
            </div>
          </details>
          <p className="text-sm text-[var(--text-secondary)]">{preview.unchanged ? "内容一致，可直接查看现有版本。" : "安装或更新后，可在清单中手动启用。"}</p>
        </div>
      ) : (
        <fieldset disabled={busy} className="min-w-0 space-y-3">
          <Select label="导入来源" value={format} options={pluginOnly ? pluginSources : packageSources}
            onChange={event => { setFormat(event.target.value as Format); setError(""); }} />
          {format === "github" && <>
            <Input label="仓库地址" value={repository} onChange={event => setRepository(event.target.value)} placeholder="https://github.com/owner/repo" />
            <Input label="Commit（40 位十六进制）" value={commit} onChange={event => setCommit(event.target.value)} />
            <Input label={pluginOnly ? "仓库内 ZIP 路径" : "仓库内路径（.md 或 .zip）"} value={path} onChange={event => setPath(event.target.value)} />
          </>}
          {needsMetadata && <div className="grid gap-3 sm:grid-cols-3">
            <Input label="稳定 ID" value={id} onChange={event => setId(event.target.value)} placeholder="my.skill" />
            <Input label="名称" value={name} onChange={event => setName(event.target.value)} />
            <Input label="版本" value={version} onChange={event => setVersion(event.target.value)} />
          </div>}
          {format === "directory" && <Input label="Skill 目录绝对路径" value={directory} onChange={event => setDirectory(event.target.value)} />}
          {format === "zip" && <label className="block text-sm">
            选择文件
            <input type="file" className="mt-2 block w-full" accept=".zip" onChange={event => setFile(event.target.files?.[0] ?? null)} />
            {file && <span className="mt-1 block break-words text-xs">{file.name}</span>}
          </label>}
          {format === "plugin_source" && <label className="block text-sm">
            选择插件目录
            <input type="file" {...{ webkitdirectory: "" }} multiple className="mt-2 block w-full"
              onChange={event => setFolder(Array.from(event.target.files ?? []))} />
            <span className="mt-1 block text-xs text-[var(--text-secondary)]">选择含 plugin.json 的目录，目录内文件会进入预览。</span>
            {folder.length > 0 && <span className="mt-1 block break-words text-xs">{folder[0].webkitRelativePath.split("/")[0]}</span>}
          </label>}
          {format === "markdown" && <>
            <label className="block text-sm">选择文件<input type="file" className="mt-2 block w-full" accept=".md" onChange={event => {
              const selected = event.target.files?.[0];
              if (!selected) return;
              void run(async () => {
                if (selected.size > maxFileBytes) throw new Error("Markdown 文件大小不能超过 512 KB。");
                setContent(await selected.text());
              });
            }} /></label>
            <Textarea label="Skill Markdown" value={content} onChange={event => setContent(event.target.value)} rows={5} />
          </>}
        </fieldset>
      )}
    </Modal>
  );
}
