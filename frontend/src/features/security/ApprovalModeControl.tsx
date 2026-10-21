import { useEffect, useState } from "react";
import { requestResult } from "../../core/api/http";
import { approvalModes, type ApprovalMode } from "../../components/chat/chatRequestOptions";
import { ShieldCheck } from "lucide-react";
import { ComposerPopover } from "../../components/chat/ComposerPopover";

export function ApprovalModeControl({ disabled = false }: { disabled?: boolean }) {
  const [mode, setMode] = useState<ApprovalMode | "">("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    void requestResult<{ mode: ApprovalMode }>("GET", "/api/security/approval-mode").then(result => {
      if (!active) return;
      if (result.ok) setMode(result.data.mode); else setError(result.error);
    });
    return () => { active = false; };
  }, []);
  const save = async (next: ApprovalMode) => {
    setBusy(true); setError("");
    const result = await requestResult<{ mode: ApprovalMode }>("PUT", "/api/security/approval-mode", { mode: next });
    if (result.ok) setMode(result.data.mode); else setError(result.error);
    setBusy(false);
  };
  return <ComposerPopover label={`审批模式：${approvalModes.find(item => item.value === mode)?.label || "读取中"}`} disabled={busy || disabled} trigger={<ShieldCheck size={16}/>}>
    {() => <div className="composer-menu-options" aria-label="审批模式">
      {approvalModes.map(item => <button key={item.value} type="button" disabled={!mode || busy} aria-pressed={mode === item.value} onClick={() => void save(item.value)}><span><strong>{item.label}</strong><small>{item.description}</small></span>{mode === item.value && <span aria-hidden="true">✓</span>}</button>)}
      {busy && <p role="status" className="composer-menu-hint">正在保存…</p>}
      {error && <p role="alert" className="text-[var(--danger)]">{error}</p>}
    </div>}
  </ComposerPopover>;
}
