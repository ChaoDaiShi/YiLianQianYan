import { useEffect, useState, type FormEvent } from "react";
import { Badge, Button, EmptyState, ErrorState, FormField, Input, Modal, Select, Tabs, Textarea } from "../../components/ui";
import { activateHarness, getHarnesses, saveHarnesses, type HarnessItem, type HarnessLevel, type HarnessProfile, type HarnessSettings, type HarnessSkill, type HarnessSnapshot, type HarnessToolGroup } from "../../api/harnesses";
import { formatToolDisplayName } from "../../components/chat/toolDisplay";
import HarnessProfileDetails from "./HarnessProfileDetails";

const levelNames: Record<HarnessLevel, string> = { system: "系统级", user: "用户级" };
const levels = Object.entries(levelNames).map(([value, label]) => ({ value, label }));
const defaultProfile: HarnessProfile = {
  id: "default", name: "系统默认", level: "system", tool_group_ids: [], workflow_ids: [], plugin_ids: [], agent_id: null,
};

function Choices({ label, description, items, values, onChange }: { label: string; description?: string; items: HarnessItem[]; values: string[]; onChange: (values: string[]) => void }) {
  return <fieldset className="min-w-0">
    <legend className="mb-1 text-sm font-medium">{label}</legend>
    {description && <p className="mb-2 text-xs text-[var(--text-secondary)]">{description}</p>}
    <div className="max-h-44 space-y-1 overflow-y-auto rounded-[var(--radius-md)] border border-[var(--border-soft)] p-2">
      {items.length === 0 && <p className="text-xs text-[var(--text-secondary)]">暂无可选项</p>}
      {items.map(item => <label key={item.id} className="flex items-start gap-2 text-sm">
        <input type="checkbox" checked={values.includes(item.id)} onChange={event => onChange(event.target.checked
          ? [...values, item.id] : values.filter(id => id !== item.id))} />
        <span className="min-w-0 break-words">{item.name}</span>
      </label>)}
    </div>
  </fieldset>;
}

type Editor = { kind: "profile"; value: HarnessProfile } | { kind: "group"; value: HarnessToolGroup };

function choiceNames(ids: string[], items: HarnessItem[]) {
  return ids.map(id => items.find(item => item.id === id)?.name ?? id).join("、") || "未选择";
}

function SkillChoices({ profile, skills, onChange }: {
  profile: HarnessProfile; skills: HarnessSkill[]; onChange: (ids: string[] | null) => void;
}) {
  const selected = profile.skill_ids;
  const available = skills.filter(skill => !skill.plugin_id || profile.plugin_ids.includes(skill.plugin_id));
  const items = available.map(skill => ({ id: skill.id, name: skill.name === skill.id ? skill.name : `${skill.name} · ${skill.id}` }));
  for (const id of selected ?? []) {
    if (available.some(skill => skill.id === id)) continue;
    const name = skills.find(skill => skill.id === id)?.name ?? id;
    items.push({ id, name: `${name}（当前不可用）` });
  }
  return <div className="space-y-2">
    <Select label="技能范围" options={[{ value: "all", label: "全部可用技能" }, { value: "selected", label: "仅所选技能" }]}
      value={selected == null ? "all" : "selected"} onChange={event => onChange(event.target.value === "all" ? null : [])} />
    {selected != null && <Choices label="技能" description="留空时不使用技能。插件技能在选择来源插件后可选。"
      items={items} values={selected} onChange={onChange} />}
  </div>;
}

export default function HarnessManager() {
  const [snapshot, setSnapshot] = useState<HarnessSnapshot | null>(null);
  const [tab, setTab] = useState("profiles");
  const [editor, setEditor] = useState<Editor | null>(null);
  const [viewedProfileId, setViewedProfileId] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [query, setQuery] = useState("");

  async function load() {
    setError("");
    const result = await getHarnesses();
    if (result.ok) setSnapshot(result.data);
    else setError(result.error);
  }
  useEffect(() => { void load(); }, []);

  async function save(settings: HarnessSettings, activate = false) {
    if (busy) return;
    setBusy(true); setError("");
    const result = await (activate ? activateHarness(settings.active_id, settings.revision) : saveHarnesses(settings));
    if (result.ok) {
      setSnapshot(current => current && { ...current, settings: result.data });
      setEditor(null);
    } else setError(result.error);
    setBusy(false);
  }

  if (!snapshot) return error ? <ErrorState title="Harness 暂时无法读取" description={error} onRetry={() => void load()} /> : <p role="status">正在读取 Harness…</p>;
  const { settings, system_groups, catalog } = snapshot;
  const groups = [...system_groups, ...settings.groups];
  const active = settings.profiles.find(item => item.id === settings.active_id);
  const profiles = [defaultProfile, ...settings.profiles];
  const viewedProfile = profiles.find(item => item.id === viewedProfileId);

  function open(next: Editor) { setError(""); setQuery(""); setEditor(next); }

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!editor) return;
    if (editor.kind === "group") {
      const group = { ...editor.value, name: editor.value.name.trim() };
      const groups = settings.groups.some(item => item.id === group.id) ? settings.groups.map(item => item.id === group.id ? group : item) : [...settings.groups, group];
      void save({ ...settings, groups });
    } else {
      const profile = { ...editor.value, name: editor.value.name.trim(), description: editor.value.description?.trim() };
      const profiles = settings.profiles.some(item => item.id === profile.id) ? settings.profiles.map(item => item.id === profile.id ? profile : item) : [...settings.profiles, profile];
      void save({ ...settings, profiles });
    }
  }

  return <section className="flex min-h-0 min-w-0 flex-1 flex-col gap-3 overflow-hidden" aria-label="Harness 配置">
    <header className="shrink-0 space-y-3 border-b border-[var(--border-soft)] pb-3">
      <div>
        <h2 className="text-base font-semibold">Harness 能力组合</h2>
        <p className="mt-1 text-sm text-[var(--text-secondary)]">组合忆涟读取信息和执行任务时使用的技能、工具与工作流。</p>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Tabs tabs={[{ id: "profiles", label: "能力组合", panelId: false }, { id: "groups", label: "工具组", panelId: false }]} activeId={tab} onChange={setTab} variant="segment" size="sm" aria-label="Harness 管理" />
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="secondary" disabled={busy} onClick={() => void load()}>刷新可用组件</Button>
          {tab === "profiles" ? <Button size="sm" disabled={busy} onClick={() => open({ kind: "profile", value: { ...defaultProfile, id: crypto.randomUUID(), name: "", level: "user" } })}>新建 Harness</Button>
            : <Button size="sm" disabled={busy} onClick={() => open({ kind: "group", value: { id: crypto.randomUUID(), name: "", level: "user", tools: [] } })}>新建工具组</Button>}
        </div>
      </div>
    </header>
    <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-1" role="tabpanel" aria-labelledby={`tab-${tab}`}>
    {error && !editor && <ErrorState title="配置操作未完成" description={error} onRetry={() => void load()} />}
    {tab === "profiles" ? <div className="space-y-2">
      <p className="text-sm text-[var(--text-secondary)]">当前使用 {active?.name ?? "系统默认"}。切换适用于新对话轮次和新工作流运行。</p>
      {profiles.map(item => (
        <article key={item.id} className="glass-card space-y-2 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="min-w-0"><h3 className="break-words font-semibold">{item.name}</h3>
              <p className="text-xs text-[var(--text-secondary)]">{levelNames[item.level]}</p>
              {item.description && <p className="mt-1 whitespace-pre-wrap break-words text-sm text-[var(--text-secondary)]">{item.description}</p>}
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button size="sm" variant="secondary" onClick={() => setViewedProfileId(item.id)}>查看能力</Button>
              {item.id !== defaultProfile.id && <Button size="sm" variant="secondary" disabled={busy} onClick={() => open({ kind: "profile", value: item })}>编辑</Button>}
              {settings.active_id === item.id ? <Badge tone="info">当前组合</Badge> : <Button size="sm" disabled={busy} onClick={() => void save({ ...settings, active_id: item.id }, true)}>设为当前</Button>}
            </div>
          </div>
          {item.id !== defaultProfile.id
            && <dl className="grid gap-2 border-t border-[var(--border-soft)] pt-2 text-xs sm:grid-cols-2 lg:grid-cols-4">
              {[
                { label: "工具组", value: choiceNames(item.tool_group_ids, groups) },
                { label: "技能", value: item.skill_ids == null ? "全部可用技能" : item.skill_ids.length ? choiceNames(item.skill_ids, catalog.skills) : "不使用技能" },
                { label: "运行工作流", value: choiceNames(item.workflow_ids, catalog.workflows) },
                { label: "插件与技能来源", value: choiceNames(item.plugin_ids, catalog.plugins) },
                { label: "对话智能体", value: item.agent_id ? choiceNames([item.agent_id], catalog.agents) : "默认智能体" },
              ].map(({ label, value }) => <div key={label} className="min-w-0">
                <dt className="text-[var(--text-muted)]">{label}</dt>
                <dd className="mt-0.5 break-words text-[var(--text-secondary)]">{value}</dd>
              </div>)}
            </dl>}
        </article>
      ))}
    </div> : <div className="space-y-3">
      {[{ title: "默认工具组", items: system_groups, custom: false }, { title: "自定义工具组", items: settings.groups, custom: true }].map(section => <section key={section.title} className="space-y-2">
        <h3 className="text-sm font-semibold">{section.title}</h3>
        {section.items.length === 0 && <EmptyState title="暂无自定义工具组" />}
        {section.items.map(group => <article key={group.id} className="glass-card flex flex-wrap items-center justify-between gap-2 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-3">
          <div className="min-w-0 flex-1"><h4 className="break-words font-medium">{group.name}</h4>
            <details className="text-xs text-[var(--text-secondary)]"><summary className="cursor-pointer">{levelNames[group.level]} · {group.tools.length} 个工具</summary>
              <p className="mt-1 break-words">{group.tools.map(formatToolDisplayName).join("、")}</p>
            </details></div>
          {section.custom && <Button size="sm" variant="secondary" disabled={busy} onClick={() => open({ kind: "group", value: group })}>编辑</Button>}
        </article>)}
      </section>)}
    </div>}
    </div>
    {viewedProfile && <HarnessProfileDetails profile={viewedProfile} snapshot={snapshot} onClose={() => setViewedProfileId(null)} />}
    {editor && <Modal open title={editor.kind === "profile" ? "配置 Harness" : "配置工具组"} size="lg" onClose={() => { if (!busy) setEditor(null); }}>
      <form className="space-y-3" onSubmit={submit}>
        {error && <p role="alert" className="break-words text-sm text-[var(--danger)]">{error}</p>}
        <fieldset disabled={busy} className="space-y-3">
          <FormField label="名称">{props => <Input {...props} required maxLength={100} value={editor.value.name} onChange={event => setEditor({ ...editor, value: { ...editor.value, name: event.target.value } } as Editor)} />}</FormField>
          <Select label="级别" options={levels} value={editor.value.level} onChange={event => {
            const level = event.target.value as HarnessLevel;
            setEditor(editor.kind === "group" ? { kind: "group", value: { ...editor.value, level } } : { kind: "profile", value: { ...editor.value, level, tool_group_ids: editor.value.tool_group_ids.filter(id => level === "user" || groups.find(group => group.id === id)?.level === "system") } });
          }} />
          {editor.kind === "group" ? <>
            <Input aria-label="搜索可选工具" placeholder="搜索工具" value={query} onChange={event => setQuery(event.target.value)} />
            <Choices label="工具" items={catalog.tools.map(tool => ({ ...tool, name: `${formatToolDisplayName(tool.id)} · ${tool.id}` })).filter(tool => tool.name.toLowerCase().includes(query.toLowerCase()))} values={editor.value.tools} onChange={tools => setEditor({ kind: "group", value: { ...editor.value, tools } })} />
          </> : <>
            <Textarea label="用途与适用场景" rows={2} maxLength={1000} value={editor.value.description ?? ""}
              onChange={event => setEditor({ kind: "profile", value: { ...editor.value, description: event.target.value } })} />
            <Choices label="工具组" description="要调用技能，请选择包含「加载技能」的工具组。" items={groups.filter(group => editor.value.level === "user" || group.level === "system").map(group => ({ id: group.id, name: `${group.name} · ${levelNames[group.level]}` }))} values={editor.value.tool_group_ids} onChange={tool_group_ids => setEditor({ kind: "profile", value: { ...editor.value, tool_group_ids } })} />
            <div className="grid gap-3 sm:grid-cols-2">
              <Choices label="运行工作流" items={catalog.workflows} values={editor.value.workflow_ids} onChange={workflow_ids => setEditor({ kind: "profile", value: { ...editor.value, workflow_ids } })} />
              <Choices label="插件与技能来源" description="调用插件提供的技能或工作流时，需要选中来源插件。" items={catalog.plugins} values={editor.value.plugin_ids} onChange={plugin_ids => setEditor({ kind: "profile", value: { ...editor.value, plugin_ids } })} />
            </div>
            <SkillChoices profile={editor.value} skills={catalog.skills} onChange={skill_ids => setEditor({ kind: "profile", value: { ...editor.value, skill_ids } })} />
            <Select label="对话智能体" options={[{ value: "", label: "使用默认智能体" }, ...catalog.agents.map(agent => ({ value: agent.id, label: agent.name }))]} value={editor.value.agent_id ?? ""} onChange={event => setEditor({ kind: "profile", value: { ...editor.value, agent_id: event.target.value || null } })} />
          </>}
        </fieldset>
        <div className="flex justify-end gap-2"><Button variant="secondary" disabled={busy} onClick={() => setEditor(null)}>取消</Button><Button type="submit" loading={busy}>保存</Button></div>
      </form>
    </Modal>}
  </section>;
}
