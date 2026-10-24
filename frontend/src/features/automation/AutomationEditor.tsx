import { useState } from "react";
import { Button, Input, Modal, Select } from "../../components/ui";
import { createAutomation, updateAutomation, type Automation, type AutomationSchedule } from "../../api/automations";
import type { WorkflowGraphRecord } from "../../api/workflows";
import type { Workspace } from "../../api/workspaces";

export const weekdays = ["一", "二", "三", "四", "五", "六", "日"];
function localDateTime() {
  const date = new Date(Date.now() + 3600000);
  const two = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())}T${two(date.getHours())}:${two(date.getMinutes())}`;
}

export default function AutomationEditor({ value, graphs, workspaces, onClose, onSaved }: {
  value: Automation | null; graphs: WorkflowGraphRecord[]; workspaces: Workspace[];
  onClose: () => void; onSaved: () => void;
}) {
  const [name, setName] = useState(value?.name ?? "");
  const [graphId, setGraphId] = useState(value?.workflow_graph_id ?? "");
  const [workspaceId, setWorkspaceId] = useState(value?.workspace_id ?? "");
  const [timezone, setTimezone] = useState(value?.timezone ?? Intl.DateTimeFormat().resolvedOptions().timeZone);
  const [kind, setKind] = useState<AutomationSchedule["kind"]>(value?.schedule.kind ?? "daily");
  const [localAt, setLocalAt] = useState(value?.schedule.kind === "once" ? value.schedule.local_at : localDateTime);
  const [time, setTime] = useState(value && value.schedule.kind !== "once" ? value.schedule.time : "09:00");
  const [days, setDays] = useState(value?.schedule.kind === "weekly" ? value.schedule.weekdays : [1, 2, 3, 4, 5]);
  const [enabled, setEnabled] = useState(value?.enabled ?? true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const isRobot = value?.target?.kind === "agent_turn";
  async function save() {
    setError("");
    const schedule: AutomationSchedule = kind === "once" ? { kind, local_at: localAt } : kind === "daily" ? { kind, time } : { kind, time, weekdays: days };
    setSaving(true);
    try {
      const input = { expected_revision: value?.revision, name: name.trim(), workflow_graph_id: graphId, target: isRobot ? value?.target : { kind: "workflow" as const, graph_id: graphId }, workspace_id: workspaceId, timezone: timezone.trim(), schedule, enabled };
      if (value) await updateAutomation(value.id, input); else await createAutomation(input);
      onSaved();
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setSaving(false); }
  }
  return <Modal open title={value ? "编辑时间任务" : "新建时间任务"} onClose={() => { if (!saving) onClose(); }} size="md" footer={<><Button variant="secondary" disabled={saving} onClick={onClose}>取消</Button><Button loading={saving} disabled={saving || !name.trim() || (!isRobot && !graphId) || !workspaceId || (kind === "weekly" && days.length === 0)} onClick={() => void save()}>保存任务</Button></>}>
    <div className="space-y-4">
      {isRobot && <p className="text-sm text-[var(--text-secondary)]">此计划执行已保存的 QQ 机器人目标。你可在消息工作台修改目标和操作范围。</p>}
      {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
      <Input label="任务名称" value={name} onChange={event => setName(event.target.value)} placeholder="例如：每周项目摘要" />
      {!isRobot && <Select label="执行工作流" value={graphId} onChange={event => setGraphId(event.target.value)} options={[{ value: "", label: "选择已有可执行工作流" }, ...graphs.map(graph => ({ value: graph.id, label: graph.name }))]} />}
      <Select label="工作空间上下文" hint="将名称和根路径提供给工作流；工具使用已保存的参数。" value={workspaceId} onChange={event => setWorkspaceId(event.target.value)} options={[{ value: "", label: "选择工作空间" }, ...workspaces.filter(item => item.status === "active").map(item => ({ value: item.id, label: item.name }))]} />
      {!isRobot && !graphs.length && <p className="text-xs text-[var(--text-secondary)]">先在工作流管理中创建可执行工作流。</p>}
      {!workspaces.some(item => item.status === "active") && <p className="text-xs text-[var(--text-secondary)]">先创建一个有效工作空间。</p>}
      <Select label="重复方式" value={kind} onChange={event => setKind(event.target.value as AutomationSchedule["kind"])} options={[{ value: "once", label: "仅一次" }, { value: "daily", label: "每天" }, { value: "weekly", label: "每周" }]} />
      <Input label="IANA 时区" value={timezone} onChange={event => setTimezone(event.target.value)} list="automation-timezones" placeholder="Asia/Shanghai" />
      <datalist id="automation-timezones"><option value="Asia/Shanghai" /><option value="Asia/Tokyo" /><option value="Europe/London" /><option value="America/New_York" /><option value="UTC" /></datalist>
      {kind === "once" ? <Input label="执行日期与时间（所选时区）" type="datetime-local" value={localAt} onChange={event => setLocalAt(event.target.value)} /> : <Input label="执行时间（所选时区）" type="time" value={time} onChange={event => setTime(event.target.value)} />}
      {kind === "weekly" && <fieldset><legend className="text-xs font-medium text-[var(--text-secondary)] mb-2">执行日</legend><div className="flex flex-wrap gap-2">{weekdays.map((day, index) => <button type="button" key={day} aria-pressed={days.includes(index + 1)} onClick={() => setDays(current => current.includes(index + 1) ? current.filter(value => value !== index + 1) : [...current, index + 1].sort())} className={`rounded-lg border px-3 py-2 text-xs ${days.includes(index + 1) ? "border-[var(--accent-primary)] bg-[var(--accent-soft)] text-[var(--accent-primary)]" : "border-[var(--border-soft)]"}`}>周{day}</button>)}</div></fieldset>}
      <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={enabled} onChange={event => setEnabled(event.target.checked)} />启用自动执行</label>
      <p className="text-xs leading-relaxed text-[var(--text-secondary)]">应用运行时按计划触发；错过时间会跳过。夏令时不存在的重复时间跳过，重复时段只执行第一次。</p>
    </div>
  </Modal>;
}
