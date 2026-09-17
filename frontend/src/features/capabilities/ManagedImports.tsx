import { useCallback, useEffect, useRef, useState } from "react";
import { managementRequest } from "../../api/productManagement";
import { Button, Input, Textarea } from "../../components/ui";

interface ManagedImport { id: string; name: string; version: string; kind: string; source: string; permissions: string[]; content_hash: string; revision: number; enabled: boolean; installed: boolean; history_count: number; runtime_ready: boolean; runtime_status: string }
interface ImportPreview { preview_id: string; package: { id: string; name: string; version: string; kind: string; permissions: string[]; content_hash: string; files: Record<string, string> }; added: string[]; changed: string[]; removed: string[]; previous_permissions: string[]; unchanged: boolean }

export default function ManagedImports() {
  const [format, setFormat] = useState("markdown");
  const [id, setId] = useState(""); const [name, setName] = useState(""); const [version, setVersion] = useState("1.0.0");
  const [content, setContent] = useState(""); const [zip, setZip] = useState("");
  const [repository, setRepository] = useState(""); const [commit, setCommit] = useState(""); const [path, setPath] = useState("SKILL.md");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [records, setRecords] = useState<ManagedImport[] | null>(null);
  const [error, setError] = useState(""); const [busy, setBusy] = useState(false); const inFlight = useRef(false);
  const reload = useCallback(async () => { const data = await managementRequest<{ imports: ManagedImport[] }>("GET", "/api/capabilities/imports"); setRecords(data.imports); }, []);
  useEffect(() => { void reload().catch(() => setError("托管导入服务 unavailable；现有能力来源不受影响。")); }, [reload]);
  const run = async (action: () => Promise<void>) => {
    if (inFlight.current) return;
    inFlight.current = true; setBusy(true); setError("");
    try { await action(); } catch (cause) { setError(cause instanceof Error ? cause.message : "操作未完成，请重试。"); }
    finally { inFlight.current = false; setBusy(false); }
  };
  const inspect = () => run(async () => {
    setPreview(null);
    const body = format === "zip" ? { format, data_base64: zip } : format === "github" ? { format, id, name, version, repository, commit, path } : { format, id, name, version, content };
    setPreview(await managementRequest<ImportPreview>("POST", "/api/capabilities/imports/inspect", body));
  });
  const change = (record: ManagedImport, action: string) => run(async () => {
    if (["uninstall", "rollback"].includes(action) && !window.confirm(`${action === "uninstall" ? "卸载" : "回滚"} ${record.name}？内容将保留有限历史，恢复后默认禁用。`)) return;
    await managementRequest(action === "uninstall" ? "DELETE" : "PUT", `/api/capabilities/imports/${encodeURIComponent(record.id)}`, action === "uninstall" ? { revision: record.revision } : { revision: record.revision, action });
    setPreview(null); await reload();
  });
  return <section id="managed-imports" className="mb-4 rounded-xl border border-[var(--border-soft)] bg-[var(--surface-elevated)] p-4" aria-label="托管能力导入">
    <h2 className="font-medium">托管 Skill 与声明式 Plugin</h2>
    <p className="my-2 text-xs text-[var(--text-secondary)]">检查 → 预览 → 确认安装 → 手动启用。导入内容作为数据存储，不加载 React、原生代码或自动运行指令。</p>
    <fieldset disabled={busy} onChange={() => setPreview(null)} className="space-y-2">
      <label className="text-sm">导入来源 <select aria-label="导入来源" value={format} onChange={(event) => { setFormat(event.target.value); setZip(""); }}><option value="markdown">Markdown Skill</option><option value="zip">ZIP 声明包</option><option value="github">公开 GitHub</option></select></label>
      {format !== "zip" && <div className="grid gap-2 md:grid-cols-3"><Input label="稳定 ID" value={id} onChange={(event) => setId(event.target.value)} placeholder="my.skill" /><Input label="名称" value={name} onChange={(event) => setName(event.target.value)} /><Input label="版本" value={version} onChange={(event) => setVersion(event.target.value)} /></div>}
      {format === "github" ? <><Input label="仓库地址" value={repository} onChange={(event) => setRepository(event.target.value)} placeholder="https://github.com/owner/repo" /><Input label="不可变 commit（40 位十六进制）" value={commit} onChange={(event) => setCommit(event.target.value)} /><Input label="仓库内明确路径（.md 或 .zip）" value={path} onChange={(event) => setPath(event.target.value)} /><p className="text-xs">仅公开仓库，拒绝重定向、私有网络与凭据 URL；本页不接收私人 token。</p></> : <label className="block text-sm">选择文件 <input type="file" accept={format === "zip" ? ".zip" : ".md"} onChange={(event) => { const file = event.target.files?.[0]; if (!file) return; void run(async () => { if (file.size > (format === "zip" ? 2 * 1024 * 1024 : 512 * 1024)) throw new Error("文件超出导入上限。"); if (format === "markdown") setContent(await file.text()); else { const bytes = new Uint8Array(await file.arrayBuffer()); let binary = ""; for (let offset = 0; offset < bytes.length; offset += 8192) binary += String.fromCharCode(...bytes.subarray(offset, offset + 8192)); setZip(btoa(binary)); } }); }} /></label>}
      {format === "markdown" && <Textarea label="Skill Markdown（仅作为数据预览）" value={content} onChange={(event) => setContent(event.target.value)} rows={4} />}
      {format === "zip" && <p className="text-xs">ZIP 根目录需包含 plugin.json，或 skill.json（id/name/version）与 SKILL.md；最多 64 项、展开 2 MiB，仅 Markdown/JSON/TXT 文本。</p>}
      <Button disabled={busy || (format === "zip" ? !zip : !id || !name)} onClick={() => void inspect()}>检查并生成预览</Button>
    </fieldset>
    {preview && <div className="mt-4 rounded-lg bg-[var(--surface-muted)] p-3" aria-label="导入变更预览">
      <h3>{preview.package.name} · {preview.package.kind} · v{preview.package.version}</h3><p className="break-all text-xs">SHA-256 {preview.package.content_hash}</p>
      <p className="my-2 text-sm">新增 {preview.added.join("、") || "无"}；修改 {preview.changed.join("、") || "无"}；移除 {preview.removed.join("、") || "无"}</p>
      <p className="text-sm">原权限：{preview.previous_permissions.join("、") || "无"} → 新权限：{preview.package.permissions.join("、") || "无"}</p>
      <details className="my-2"><summary>查看完整声明内容（不会执行）</summary>{Object.entries(preview.package.files).map(([file, text]) => <div key={file}><h4>{file}</h4><pre className="max-h-52 overflow-auto whitespace-pre-wrap text-xs">{text}</pre></div>)}</details>
      <p className="mb-2 text-xs">{preview.unchanged ? "内容一致，确认不会重复安装。" : "安装或更新后默认禁用；启用不会绕过执行权限。"}</p>
      <Button disabled={busy} onClick={() => void run(async () => { await managementRequest("POST", "/api/capabilities/imports/confirm", { preview_id: preview.preview_id }); setPreview(null); await reload(); })}>确认安装或更新</Button>
    </div>}
    {error && <p role="alert" className="my-2 text-sm text-[var(--danger)]">{error}</p>}
    <div className="mt-4 space-y-2" aria-label="托管导入清单">{records === null ? <p>托管清单 unavailable</p> : records.length === 0 ? <p>暂无托管导入。</p> : records.map((record) => <article key={record.id} className="rounded-lg border border-[var(--border-soft)] p-3"><strong>{record.name}</strong><p className="text-xs">{record.kind} · v{record.version} · {record.installed ? record.enabled ? "已启用声明" : "已禁用" : "已卸载，可恢复"}</p><p className="break-all text-xs">来源：{record.source}</p><p className="text-xs">权限：{record.permissions.join("、") || "无"}；运行时：{record.runtime_ready ? "ready" : record.runtime_status}</p><div className="mt-2 flex flex-wrap gap-2">{record.installed ? <><Button size="sm" variant="secondary" disabled={busy} onClick={() => void change(record, record.enabled ? "disable" : "enable")}>{record.enabled ? "禁用" : "启用声明"}</Button><Button size="sm" variant="secondary" disabled={busy || record.history_count === 0} onClick={() => void change(record, "rollback")}>回滚上一版本</Button><Button size="sm" variant="secondary" disabled={busy} onClick={() => void change(record, "uninstall")}>卸载</Button></> : <Button size="sm" disabled={busy} onClick={() => void change(record, "restore")}>恢复（禁用状态）</Button>}</div></article>)}</div>
  </section>;
}
