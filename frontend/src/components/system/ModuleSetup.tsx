import { useEffect, useState } from "react";
import { ArrowDown, ArrowUp } from "lucide-react";
import { Button } from "../ui";
import SettingsDialogLayout from "../../features/settings/sections/SettingsDialogLayout";
import { getNavEntryById, orderNavigationEntries } from "../layout/navRegistry";
import { MANDATORY_NAV, TRUSTED_MODULES, type ProductPreferences } from "./productPreferences";
import { useProductPreferences } from "./useProductPreferences";

interface ModuleSettingsEditorProps {
  onClose: () => void;
  onSaved?: () => void;
  title?: string;
}

export function ModuleSettingsEditor({ onClose, onSaved, title = "导航布局" }: ModuleSettingsEditorProps) {
  const { preferences, loaded, error, save } = useProductPreferences();
  const [draft, setDraft] = useState<ProductPreferences>(preferences);
  const [saving, setSaving] = useState(false);
  useEffect(() => { if (loaded) setDraft(preferences); }, [loaded, preferences]);

  const entries = draft.navigation.flatMap((item) => {
    const entry = getNavEntryById(item.id);
    return entry ? [entry] : [];
  });
  const sidebar = orderNavigationEntries(entries.filter((entry) => entry.primary));
  const moreEntries = entries.filter((entry) => !entry.primary && entry.id !== "settings" && entry.id !== "account");
  const visible = new Map(draft.navigation.map((item) => [item.id, item.visible]));
  const disabled = saving || !loaded;
  const unsaved = loaded && JSON.stringify(draft.navigation) !== JSON.stringify(preferences.navigation);

  function changeVisibility(id: string, checked: boolean) {
    setDraft((current) => ({ ...current, navigation: current.navigation.map((item) => item.id === id ? { ...item, visible: checked } : item) }));
  }

  function move(id: string, targetId: string) {
    setDraft((current) => {
      const navigation = [...current.navigation];
      const from = navigation.findIndex((item) => item.id === id);
      const to = navigation.findIndex((item) => item.id === targetId);
      [navigation[from], navigation[to]] = [navigation[to], navigation[from]];
      return { ...current, navigation };
    });
  }

  async function saveModuleSetup() {
    setSaving(true);
    try {
      await save({ ...draft, setup_completed: preferences.setup_completed });
      onSaved?.();
      onClose();
    } catch {
      // The preferences hook supplies the error while this draft stays open.
    } finally {
      setSaving(false);
    }
  }

  return (
    <SettingsDialogLayout title={title} onClose={onClose} closeDisabled={saving} unsaved={unsaved} error={error}
      action={<Button disabled={disabled} onClick={() => void saveModuleSetup()}>{saving ? "保存中…" : "确认并保存"}</Button>}>
        {!loaded && !error && <p className="settings-help" role="status">正在读取导航布局…</p>}
        <section aria-label="侧栏显示与顺序">
          <h4 className="mb-2 text-sm font-semibold">侧栏显示与顺序</h4>
          <ul className="divide-y divide-[var(--border-soft)] rounded-lg border border-[var(--border-soft)]">
            {sidebar.map((entry, index) => (
              <li key={entry.id} className="flex min-h-11 items-center justify-between gap-2 px-3 py-1 text-sm">
                <label className="flex min-w-0 flex-1 items-center gap-2">
                  <input type="checkbox" className="accent-[var(--accent-soft-fg)]" checked={visible.get(entry.id) === true}
                    disabled={disabled || MANDATORY_NAV.includes(entry.id)} onChange={(event) => changeVisibility(entry.id, event.target.checked)} />
                  {entry.label}
                </label>
                {entry.id === "chat" ? <span className="settings-help">固定在首位</span> : (
                  <div className="flex shrink-0 gap-1">
                    <Button variant="ghost" size="sm" iconOnly aria-label={"上移 " + entry.label}
                      disabled={disabled || index === 1} onClick={() => move(entry.id, sidebar[index - 1].id)}><ArrowUp size={15} /></Button>
                    <Button variant="ghost" size="sm" iconOnly aria-label={"下移 " + entry.label}
                      disabled={disabled || index === sidebar.length - 1} onClick={() => move(entry.id, sidebar[index + 1].id)}><ArrowDown size={15} /></Button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        </section>

        <details className="u2-disclosure">
          <summary>高级选项</summary>
          <div className="space-y-4 pt-2">
            <section aria-label="更多页面入口">
              <h4 className="mb-2 text-sm font-semibold">「更多」页面入口</h4>
              <div className="grid gap-2 sm:grid-cols-2">
                {moreEntries.map((entry) => (
                  <label key={entry.id} className="flex items-center gap-2 text-sm">
                    <input type="checkbox" className="accent-[var(--accent-soft-fg)]" disabled={disabled}
                      checked={visible.get(entry.id) === true} onChange={(event) => changeVisibility(entry.id, event.target.checked)} />
                    {entry.label}
                  </label>
                ))}
              </div>
            </section>
            <section aria-label="内置模块">
              <h4 className="mb-2 text-sm font-semibold">内置模块</h4>
              <div className="grid gap-2 sm:grid-cols-2">
                {TRUSTED_MODULES.map((module) => (
                  <label key={module.id} className="flex items-center gap-2 text-sm">
                    <input type="checkbox" className="accent-[var(--accent-soft-fg)]" disabled={disabled || module.mandatory}
                      checked={draft.enabled_modules.includes(module.id)} onChange={(event) => {
                        const checked = event.target.checked;
                        setDraft((current) => ({
                          ...current,
                          enabled_modules: checked ? [...current.enabled_modules, module.id] : current.enabled_modules.filter((id) => id !== module.id),
                        }));
                      }} />
                    {module.name}{module.mandatory && <span className="settings-help">必需</span>}
                  </label>
                ))}
              </div>
            </section>
          </div>
        </details>
    </SettingsDialogLayout>
  );
}

export default function ModuleSetup() {
  const { preferences } = useProductPreferences();
  const [open, setOpen] = useState(false);
  const close = () => setOpen(false);
  return <>
    <button type="button" className="shrink-0 rounded-lg p-1 text-[10px] hover:bg-white/10" aria-label="配置系统模块与导航" onClick={() => setOpen(true)}>定制导航</button>
    {open && <ModuleSettingsEditor onClose={close} title={preferences.setup_completed ? "系统模块与导航" : "选择可选系统模块"} />}
  </>;
}
