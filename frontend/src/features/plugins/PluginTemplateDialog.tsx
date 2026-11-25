import { useState } from "react";
import { managementRequest } from "../../api/productManagement";
import { Button, Input, Modal } from "../../components/ui";
import { formatImportError } from "../capabilities/PackageImportDialog";

export default function PluginTemplateDialog({ onClose }: { onClose: () => void }) {
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [version, setVersion] = useState("0.1.0");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [downloaded, setDownloaded] = useState(false);

  async function download() {
    setBusy(true);
    setError("");
    setDownloaded(false);
    try {
      const result = await managementRequest<{ data_base64: string }>("POST", "/api/capabilities/plugins/template", {
        id: id.trim(), name: name.trim(), version: version.trim(),
      });
      const bytes = Uint8Array.from(atob(result.data_base64), character => character.charCodeAt(0));
      const url = URL.createObjectURL(new Blob([bytes], { type: "application/zip" }));
      const link = document.createElement("a");
      link.href = url;
      link.download = id.trim() + "-" + version.trim() + ".zip";
      link.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
      setDownloaded(true);
    } catch (cause) {
      setError(formatImportError(cause));
    } finally {
      setBusy(false);
    }
  }

  return <Modal open title="开发插件" size="md" onClose={() => { if (!busy) onClose(); }} footer={
    <div className="settings-dialog-footer">
      {error && <p className="settings-save-error settings-dialog-error" role="alert">{error}</p>}
      <div className="settings-dialog-actions">
        <Button variant="secondary" disabled={busy} onClick={onClose}>关闭</Button>
        <Button disabled={busy || !id.trim() || !name.trim() || !version.trim()} onClick={() => void download()}>{busy ? "正在生成…" : "下载模板 ZIP"}</Button>
      </div>
    </div>
  }>
    <fieldset disabled={busy} className="space-y-3">
      <Input label="插件 ID" value={id} onChange={event => setId(event.target.value)} placeholder="example.my-plugin" />
      <Input label="名称" value={name} onChange={event => setName(event.target.value)} />
      <Input label="版本" value={version} onChange={event => setVersion(event.target.value)} />
      <p className="text-sm text-[var(--text-secondary)]">模板包含 plugin.json、示例技能和开发说明。解压编辑后，可直接从「导入插件」选择本地插件目录检查。</p>
      {downloaded && <p className="text-sm" role="status">已生成模板 ZIP。</p>}
    </fieldset>
  </Modal>;
}
