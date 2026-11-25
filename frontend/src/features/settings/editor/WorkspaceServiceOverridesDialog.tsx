import { useEffect, useRef, useState, type RefObject } from "react";
import { Button, Select } from "../../../components/ui";
import { listWorkspaces } from "../../../api/workspaces";
import {
  listWorkspaceServiceOverrides,
  loadWorkspaceServiceCatalog,
  loadWorkspaceServices,
  refreshWorkspaceServiceDraft,
  saveWorkspaceService,
  serviceChoices,
  serviceSourceKey,
  workspaceServiceKinds,
  workspaceServiceLabels,
  type ServiceSource,
  type WorkspaceServiceDraft,
  type WorkspaceServiceOverrides,
  type WorkspaceServiceSettings,
  type WorkspaceServiceKind,
} from "../model/workspaceServices";
import SettingsDialogLayout from "../sections/SettingsDialogLayout";

type WorkspaceRef = { id: string; name: string };

function workspaceFromOverride(entry: WorkspaceServiceOverrides): WorkspaceRef {
  return { id: entry.workspace_id, name: entry.name };
}

interface Props {
  initialWorkspace?: WorkspaceRef;
  onClose: () => void;
}

export default function WorkspaceServiceOverridesDialog({ initialWorkspace, onClose }: Props) {
  const returnFocusRef = useRef<HTMLElement>(typeof document !== "undefined" && document.activeElement instanceof HTMLElement ? document.activeElement : null);
  const [workspace, setWorkspace] = useState<WorkspaceRef | null>(initialWorkspace ?? null);
  const [overrides, setOverrides] = useState<WorkspaceServiceOverrides[] | null>(null);
  const [choices, setChoices] = useState<WorkspaceRef[] | null>(null);
  const [selectedId, setSelectedId] = useState("");
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState("");
  const [reload, setReload] = useState(0);

  useEffect(() => {
    if (workspace) return;
    let active = true;
    setError("");
    if (adding) {
      setChoices(null);
      void listWorkspaces().then(result => {
        if (!active) return;
        if (result.ok) setChoices(result.data.filter(item => item.status === "active"));
        else setError(result.error);
      });
    } else {
      setOverrides(null);
      void listWorkspaceServiceOverrides().then(result => {
        if (!active) return;
        if (result.ok) setOverrides(result.data.workspaces);
        else setError(result.error);
      });
    }
    return () => { active = false; };
  }, [workspace, adding, reload]);

  if (workspace) {
    return <WorkspaceServiceEditor key={workspace.id} workspace={workspace} onClose={onClose} returnFocusRef={returnFocusRef}
      onBack={initialWorkspace ? undefined : () => { setWorkspace(null); setAdding(false); }} />;
  }

  function openWorkspace(item: WorkspaceRef) {
    setAdding(false);
    setWorkspace(item);
  }

  const selected = choices?.find(item => item.id === selectedId);
  const action = adding
    ? <Button disabled={!selected} onClick={() => { if (selected) openWorkspace(selected); }}>继续</Button>
    : <Button onClick={() => { setSelectedId(""); setAdding(true); }}>添加覆盖</Button>;
  const secondaryActions = error
    ? <Button variant="secondary" onClick={() => setReload(value => value + 1)}>重新加载</Button>
    : adding && <Button variant="secondary" onClick={() => setAdding(false)}>返回</Button>;

  return <SettingsDialogLayout title="工作空间模型覆盖" onClose={onClose} returnFocusRef={returnFocusRef} error={error} action={action} secondaryActions={secondaryActions}>
    {adding ? (
      choices?.length ? <Select label="工作空间" value={selectedId} onChange={event => setSelectedId(event.target.value)}
        options={[{ value: "", label: "请选择工作空间" }, ...choices.map(item => ({ value: item.id, label: item.name }))]} />
        : choices ? <p className="settings-help">没有可配置的工作空间。</p>
        : !error && <p className="settings-help" role="status">正在读取工作空间…</p>
    ) : (
      <>
        <p className="settings-help">工作空间默认继承全局对话、图像理解和图像生成模型。</p>
        {overrides?.length ? <ul className="settings-overview-list">
          {overrides.map(item => <li key={item.workspace_id} className="border-b border-[var(--border-soft)] last:border-b-0">
            <button className="settings-overview-entry" type="button" onClick={() => openWorkspace(workspaceFromOverride(item))}>
              <span className="flex min-w-0 flex-wrap justify-between gap-x-2 gap-y-1">
                <span className="min-w-0 break-words">{item.name}</span>
                <span className="settings-help">{item.kinds.map(kind => workspaceServiceLabels[kind]).join("、")}</span>
              </span>
            </button>
          </li>)}
        </ul> : overrides ? <p className="settings-help">暂无工作空间覆盖。</p>
          : !error && <p className="settings-help" role="status">正在读取模型覆盖…</p>}
      </>
    )}
  </SettingsDialogLayout>;
}

function WorkspaceServiceEditor({ workspace, onClose, onBack, returnFocusRef }: {
  workspace: WorkspaceRef;
  onClose: () => void;
  onBack?: () => void;
  returnFocusRef: RefObject<HTMLElement>;
}) {
  const [settings, setSettings] = useState<WorkspaceServiceSettings | null>(null);
  const [catalog, setCatalog] = useState<Awaited<ReturnType<typeof loadWorkspaceServiceCatalog>> | null>(null);
  const [draft, setDraft] = useState<WorkspaceServiceDraft>({});
  const [busy, setBusy] = useState(false);
  const [needsReload, setNeedsReload] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  function applySettings(latest: WorkspaceServiceSettings) {
    setSettings(latest);
    setDraft(current => refreshWorkspaceServiceDraft(current, latest));
  }

  async function refresh() {
    setBusy(true);
    setError("");
    try {
      const [latest, choices] = await Promise.all([loadWorkspaceServices(workspace.id), loadWorkspaceServiceCatalog()]);
      if (!latest.ok) throw new Error(latest.error);
      applySettings(latest.data);
      setCatalog(choices);
      setNeedsReload(false);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setNeedsReload(true);
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => { void refresh(); }, [workspace.id]);

  function edit(kind: WorkspaceServiceKind, value: string) {
    if (!settings) return;
    const source: ServiceSource | null = value === "" ? null : {
      source: value.startsWith("profile:") ? "profile" : "pool",
      id: value.slice(value.indexOf(":") + 1),
    };
    setNotice("");
    setDraft(current => refreshWorkspaceServiceDraft({ ...current, [kind]: source }, settings));
  }

  async function save(kind: WorkspaceServiceKind) {
    const changed = draft[kind];
    if (!settings || changed === undefined) return;
    setBusy(true);
    setError("");
    setNotice("");
    const saved = await saveWorkspaceService(workspace.id, kind, settings[kind].revision, changed);
    if (!saved.ok) {
      if (saved.status === 409) {
        setNeedsReload(true);
        setError("配置已被修改。重新加载可查看最新的已保存配置，你的修改会保留。");
      } else {
        setError(saved.error);
      }
      setBusy(false);
      return;
    }

    applySettings({ ...settings, [kind]: saved.data });
    const latest = await loadWorkspaceServices(workspace.id);
    if (latest.ok) {
      applySettings(latest.data);
      setNotice(changed === null ? "已恢复继承全局设置。" : "已保存覆盖，下次调用生效。");
    } else {
      setNeedsReload(true);
      setError(`已保存，刷新工作空间配置失败。${latest.error}`);
    }
    setBusy(false);
  }

  const dirty = Object.keys(draft).length > 0;
  return <SettingsDialogLayout title="工作空间模型覆盖" onClose={onClose} closeDisabled={busy} returnFocusRef={returnFocusRef} unsaved={dirty} error={error} notice={notice}
    secondaryActions={<>
      {onBack && <Button variant="secondary" disabled={busy || dirty} onClick={onBack}>返回覆盖列表</Button>}
      {needsReload && <Button variant="secondary" disabled={busy} onClick={() => void refresh()}>重新加载</Button>}
    </>}>
    <div className="min-w-0 break-words">
      <h3 className="text-sm font-semibold">{workspace.name}</h3>
      <p className="settings-help">适用于这个工作空间的对话、识图和生图，变更在下次调用时生效。单次对话选择和智能体单独配置的模型优先。</p>
      <p className="settings-help">语音使用全局会话设置；向量模型由统一资料索引管理。</p>
    </div>
    {!settings || !catalog ? (!error && <p className="settings-help" role="status">正在读取模型配置…</p>) : workspaceServiceKinds.map(kind => {
      const current = settings[kind];
      const label = workspaceServiceLabels[kind];
      const edited = draft[kind];
      const selected = serviceSourceKey(edited === undefined ? current.override : edited);
      const options = serviceChoices(kind, catalog);
      const unavailable = selected && !options.some(item => item.value === selected);
      const effectiveLabel = current.effective
        ? `${current.effective.label}（${current.effective.scope === "workspace" ? "工作空间覆盖" : "继承全局"}）`
        : current.override ? "当前覆盖不可用，请选择其他来源或恢复继承" : "全局来源未配置或不可用";
      return <section key={kind} aria-label={label} className="space-y-2 rounded-md border border-[var(--border-soft)] p-3">
        <div className="space-y-1 break-words text-sm">
          <p>当前来源：{effectiveLabel}</p>
          {current.override && <p className="settings-help">继承来源：{current.inherited?.label ?? "全局来源未配置或不可用"}</p>}
        </div>
        <Select label={label} value={selected} disabled={busy} onChange={event => edit(kind, event.target.value)} options={[
          { value: "", label: "继承全局" }, ...options,
          ...(unavailable ? [{ value: selected, label: "当前所选来源不可用", disabled: true }] : []),
        ]} />
        <div className="flex justify-end">
          <Button size="sm" disabled={busy || needsReload || edited === undefined || Boolean(unavailable)} onClick={() => void save(kind)}>
            {edited === null ? "恢复继承" : "保存覆盖"}
          </Button>
        </div>
      </section>;
    })}
  </SettingsDialogLayout>;
}
