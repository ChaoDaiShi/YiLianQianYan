import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button, Modal } from "../ui";
import { NAV_GROUPS } from "../layout/navGroups";
import { MANDATORY_NAV, TRUSTED_MODULES, type ProductPreferences } from "./productPreferences";
import { useProductPreferences } from "./useProductPreferences";
import { getProviderReadiness } from "../../api/providerConnection";
import type { ProviderReadinessProjection } from "../../types";

export default function ModuleSetup() {
  const navigate = useNavigate();
  const { preferences, loaded, error, save } = useProductPreferences();
  const [open, setOpen] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [draft, setDraft] = useState<ProductPreferences>(preferences);
  const [saving, setSaving] = useState(false);
  const [providerStepOpen, setProviderStepOpen] = useState(false);
  const [readiness, setReadiness] = useState<ProviderReadinessProjection | null>(null);
  useEffect(() => { if (loaded && !preferences.setup_completed && !dismissed) { setDraft(preferences); setOpen(true); } }, [loaded, preferences, dismissed]);
  const close = () => { setDismissed(true); setOpen(false); };
  const saveModuleSetup = async () => {
    const firstRun = !preferences.setup_completed;
    setSaving(true);
    try {
      await save({ ...draft, setup_completed: true });
      close();
      setProviderStepOpen(firstRun);
    } catch {
      // The hook exposes the safe, user-facing error and keeps the dialog open.
    } finally {
      setSaving(false);
    }
  };
  useEffect(() => {
    if (!providerStepOpen) return;
    void getProviderReadiness().then(setReadiness);
  }, [providerStepOpen]);
  return <>
    <button type="button" className="shrink-0 rounded-lg p-1 text-[10px] hover:bg-white/10" aria-label="配置系统模块与导航" onClick={() => { setDraft(preferences); setOpen(true); }}>定制导航</button>
    <Modal open={open} onClose={close} title={preferences.setup_completed ? "系统模块与导航" : "选择可选系统模块"}>
      <p className="mb-3 text-sm">内置声明式模块不会加载第三方代码。取消保留安全默认设置；安全、审批和恢复始终可达。</p>
      {TRUSTED_MODULES.map((module) => <label key={module.id} className="mb-2 block text-sm"><input type="checkbox" disabled={module.mandatory || saving} checked={draft.enabled_modules.includes(module.id)} onChange={(event) => setDraft({ ...draft, enabled_modules: event.target.checked ? [...draft.enabled_modules, module.id] : draft.enabled_modules.filter((id) => id !== module.id) })} /> {module.name} · v{module.version} · 内置{module.mandatory ? "（必需）" : ""}<span className="block text-xs text-[var(--text-faint)]">{module.permissions.join("、")}</span></label>)}
      <h3 className="mb-2 mt-4 font-medium">侧栏显示与顺序</h3>
      {draft.navigation.map((item, index) => <div key={item.id} className="mb-1 flex items-center gap-2 text-sm">
        <label className="flex-1"><input type="checkbox" disabled={MANDATORY_NAV.includes(item.id) || saving} checked={item.visible} onChange={(event) => setDraft({ ...draft, navigation: draft.navigation.map((entry) => entry.id === item.id ? { ...entry, visible: event.target.checked } : entry) })} /> {NAV_GROUPS.flatMap((group) => group.items).find((entry) => entry.id === item.id)?.label}</label>
        <button type="button" disabled={index === 0 || saving} aria-label={`上移 ${item.id}`} onClick={() => { const navigation = [...draft.navigation]; [navigation[index - 1], navigation[index]] = [navigation[index], navigation[index - 1]]; setDraft({ ...draft, navigation }); }}>↑</button>
        <button type="button" disabled={index === draft.navigation.length - 1 || saving} aria-label={`下移 ${item.id}`} onClick={() => { const navigation = [...draft.navigation]; [navigation[index + 1], navigation[index]] = [navigation[index], navigation[index + 1]]; setDraft({ ...draft, navigation }); }}>↓</button>
      </div>)}
      {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
      <div className="mt-4 flex gap-2"><Button disabled={saving || !loaded} onClick={() => void saveModuleSetup()}>确认并保存</Button><Button variant="secondary" disabled={saving} onClick={close}>取消</Button></div>
    </Modal>
    <Modal
      open={providerStepOpen}
      onClose={() => setProviderStepOpen(false)}
      title="开始使用忆涟"
      footer={<Button variant="secondary" onClick={() => setProviderStepOpen(false)}>稍后设置</Button>}
    >
      <div className="space-y-3">
        <section className="rounded-lg border border-[var(--border-soft)] p-3">
          <div className="flex items-center justify-between gap-3">
            <div><p className="text-sm font-medium">模型服务</p><p className="text-xs text-[var(--text-muted)]">{readiness?.model.available ? "● 已可用" : "○ 未配置或不可用"} · AI 功能推荐配置</p></div>
            <Button size="sm" onClick={() => { setProviderStepOpen(false); navigate("/settings?section=model"); }}>去配置</Button>
          </div>
        </section>
        <section className="rounded-lg border border-[var(--border-soft)] p-3">
          <div className="flex items-center justify-between gap-3">
            <div><p className="text-sm font-medium">语音服务（可选）</p><p className="text-xs text-[var(--text-muted)]">{readiness?.stt.available && readiness?.tts.available ? "● 已可用" : "○ 未配置或部分不可用"} · 不阻止进入应用</p></div>
            <Button size="sm" variant="secondary" onClick={() => { setProviderStepOpen(false); navigate("/settings?section=voice"); }}>去配置</Button>
          </div>
        </section>
      </div>
    </Modal>
  </>;
}
