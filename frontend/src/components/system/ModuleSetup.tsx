import { useEffect, useState } from "react";
import { Button, Modal } from "../ui";
import { NAV_GROUPS } from "../layout/navGroups";
import { MANDATORY_NAV, TRUSTED_MODULES, type ProductPreferences } from "./productPreferences";
import { useProductPreferences } from "./useProductPreferences";

/** Shared inline form for the toolbar dialog and the settings canvas node. */
export function ModuleSettingsEditor({ onClose, onSaved }: { onClose: () => void; onSaved?: () => void }) {
  const { preferences, loaded, error, save } = useProductPreferences();
  const [draft, setDraft] = useState<ProductPreferences>(preferences);
  const [saving, setSaving] = useState(false);
  useEffect(() => { if (loaded) setDraft(preferences); }, [loaded, preferences]);
  const close = onClose;
  const saveModuleSetup = async () => {
    setSaving(true);
    try {
      await save({ ...draft, setup_completed: preferences.setup_completed });
      onSaved?.();
      close();
    } catch {
      // The hook exposes the service error and keeps the editor open.
    } finally {
      setSaving(false);
    }
  };
  return <>
      <p className="mb-3 text-sm">内置声明式模块不会加载第三方代码。取消保留安全默认设置；安全、审批和恢复始终可达。</p>
      {TRUSTED_MODULES.map((module) => <label key={module.id} className="mb-2 block text-sm"><input type="checkbox" disabled={module.mandatory || saving || !loaded} checked={draft.enabled_modules.includes(module.id)} onChange={(event) => setDraft({ ...draft, enabled_modules: event.target.checked ? [...draft.enabled_modules, module.id] : draft.enabled_modules.filter((id) => id !== module.id) })} /> {module.name} · v{module.version} · 内置{module.mandatory ? "（必需）" : ""}<span className="block text-xs text-[var(--text-faint)]">{module.permissions.join("、")}</span></label>)}
      <h3 className="mb-2 mt-4 font-medium">侧栏显示与顺序</h3>
      {draft.navigation.map((item, index) => <div key={item.id} className="mb-1 flex items-center gap-2 text-sm">
        <label className="flex-1"><input type="checkbox" disabled={MANDATORY_NAV.includes(item.id) || saving || !loaded} checked={item.visible} onChange={(event) => setDraft({ ...draft, navigation: draft.navigation.map((entry) => entry.id === item.id ? { ...entry, visible: event.target.checked } : entry) })} /> {NAV_GROUPS.flatMap((group) => group.items).find((entry) => entry.id === item.id)?.label}</label>
        <button type="button" disabled={index === 0 || saving || !loaded} aria-label={`上移 ${item.id}`} onClick={() => { const navigation = [...draft.navigation]; [navigation[index - 1], navigation[index]] = [navigation[index], navigation[index - 1]]; setDraft({ ...draft, navigation }); }}>↑</button>
        <button type="button" disabled={index === draft.navigation.length - 1 || saving || !loaded} aria-label={`下移 ${item.id}`} onClick={() => { const navigation = [...draft.navigation]; [navigation[index + 1], navigation[index]] = [navigation[index], navigation[index + 1]]; setDraft({ ...draft, navigation }); }}>↓</button>
      </div>)}
      {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
      <div className="mt-4 flex gap-2"><Button disabled={saving || !loaded} onClick={() => void saveModuleSetup()}>确认并保存</Button><Button variant="secondary" disabled={saving} onClick={close}>取消</Button></div>
  </>;
}

export default function ModuleSetup() {
  const { preferences } = useProductPreferences();
  const [open, setOpen] = useState(false);
  const close = () => setOpen(false);
  return <>
    <button type="button" className="shrink-0 rounded-lg p-1 text-[10px] hover:bg-white/10" aria-label="配置系统模块与导航" onClick={() => setOpen(true)}>定制导航</button>
    <Modal open={open} onClose={close} title={preferences.setup_completed ? "系统模块与导航" : "选择可选系统模块"}>
      <ModuleSettingsEditor onClose={close} />
    </Modal>
  </>;
}
