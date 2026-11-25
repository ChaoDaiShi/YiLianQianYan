import { useRef, useState } from "react";
import { Button, Input, Modal, Select, Textarea } from "../../components/ui";
import { createAutomation, updateAutomation, type Automation, type AutomationSchedule } from "../../api/automations";
import type { WorkflowGraphRecord } from "../../api/workflows";
import type { Workspace } from "../../api/workspaces";
import { automationManagementPolicy } from "./automationManagementPolicy";
import { prospectiveWorkspaceRoot, validateAutomationDraft } from "./automationValidation";
import AttentionFields, { defaultAttentionSchedule, localDateTime } from "./AttentionFields";

export const weekdays = ["一", "二", "三", "四", "五", "六", "日"];

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
  const [time, setTime] = useState(value && (value.schedule.kind === "daily" || value.schedule.kind === "weekly") ? value.schedule.time : "09:00");
  const [days, setDays] = useState(value?.schedule.kind === "weekly" ? value.schedule.weekdays : [1, 2, 3, 4, 5]);
  const [filePath, setFilePath] = useState(value?.schedule.kind === "file_changed" ? value.schedule.path : "");
  const [watch, setWatch] = useState(() => value?.schedule.kind === "watch" ? value.schedule : defaultAttentionSchedule());
  const [enabled, setEnabled] = useState(value?.enabled ?? true);
  const [saving, setSaving] = useState(false);
  const savingRef = useRef(false);
  const [error, setError] = useState("");
  const personal = value?.target?.kind === "personal_assistant" ? value.target : null;
  const [targetKind, setTargetKind] = useState(personal ? "personal_assistant" : "workflow");
  const [goal, setGoal] = useState(personal?.goal ?? "");
  const [allowDiscovery, setAllowDiscovery] = useState(personal?.allow_file_discovery ?? false);
  const [allowWrites, setAllowWrites] = useState(personal?.allow_file_writes ?? false);
  const [maxRuns, setMaxRuns] = useState(personal?.max_runs ?? 5);
  const isPersonal = targetKind === "personal_assistant";
  const isRobot = value?.target?.kind === "agent_turn";
  const schedule: AutomationSchedule = kind === "watch" ? {
    ...watch,
    source: watch.source.kind === "file"
      ? { ...watch.source, path: watch.source.path.trim() }
      : { ...watch.source, url: watch.source.url.trim() },
  } : kind === "file_changed" ? { kind, path: filePath.trim() } : kind === "once" ? { kind, local_at: localAt } : kind === "daily" ? { kind, time } : { kind, time, weekdays: days };
  const validation = validateAutomationDraft({ name, targetKind: isRobot ? "agent_turn" : isPersonal ? "personal_assistant" : "workflow", graphId, workspaceId, timezone, schedule, goal, maxRuns, enabled }, workspaces);
  const editRestriction = value && !automationManagementPolicy(value).canEdit ? automationManagementPolicy(value).editReason : "";
  const scope = prospectiveWorkspaceRoot(value, workspaceId, workspaces);
  async function save() {
    if (savingRef.current) return;
    setError("");
    if (editRestriction || validation.length) { setError(editRestriction || validation[0]); return; }
    savingRef.current = true;
    setSaving(true);
    try {
      const target = isRobot ? value?.target : isPersonal ? { kind: "personal_assistant" as const, goal: goal.trim(), allow_file_writes: allowWrites, allow_file_discovery: allowDiscovery, max_runs: maxRuns } : { kind: "workflow" as const, graph_id: graphId };
      const input = { expected_revision: value?.revision, name: name.trim(), workflow_graph_id: isPersonal ? "" : graphId, target, workspace_id: workspaceId, timezone: timezone.trim(), schedule, enabled };
      if (value) await updateAutomation(value.id, input); else await createAutomation(input);
      onSaved();
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { savingRef.current = false; setSaving(false); }
  }
  return <Modal open title={value ? "编辑时间任务" : "新建时间任务"} onClose={() => { if (!savingRef.current) onClose(); }} size="md" footer={<><Button variant="secondary" disabled={saving} onClick={onClose}>取消</Button><Button loading={saving} disabled={saving || Boolean(editRestriction) || validation.length > 0} title={editRestriction || validation[0] || "保存任务"} onClick={() => void save()}>保存任务</Button></>}>
    <fieldset disabled={saving || Boolean(editRestriction)} className="min-w-0 space-y-4">
      {isRobot && <p className="text-sm text-[var(--text-secondary)]">此计划执行已保存的 QQ 机器人目标。你可在消息工作台修改目标和操作范围。</p>}
      {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
      {editRestriction && <p role="status" className="text-sm text-[var(--text-secondary)]">{editRestriction}</p>}
      {!error && !editRestriction && validation.length > 0 && <p role="status" className="text-xs text-[var(--text-secondary)]">保存前请完善：{validation[0]}</p>}
      <Input label="任务名称" maxLength={120} value={name} onChange={event => setName(event.target.value)} placeholder="例如：每周项目摘要" hint="最多 120 个字符。" />
      {!isRobot && <Select label="任务类型" disabled={Boolean(value)} value={targetKind} onChange={event => { setTargetKind(event.target.value); if (event.target.value !== "personal_assistant" && (kind === "file_changed" || kind === "watch")) setKind("daily"); }} options={[{ value: "workflow", label: "执行工作流" }, { value: "personal_assistant", label: "个人主动助手" }]} />}
      {isPersonal && <><Textarea label="持续跟进的目标" maxLength={8000} value={goal} onChange={event => setGoal(event.target.value)} placeholder="例如：读取工作目录里的 notes.txt，整理成一个新的 Markdown 报告并核验；遇到不明确的信息先问我。" /><Input label="委托最多执行轮次" type="number" min={1} max={20} value={maxRuns} onChange={event => setMaxRuns(Number(event.target.value))} hint="手动运行也计入总预算；每轮保留原有 Agent 循环保护。耗尽后须由你增加预算。" /><label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={allowDiscovery} onChange={event => setAllowDiscovery(event.target.checked)} />允许查找授权目录中的资料（文件名与文本）</label><p className="text-xs leading-relaxed text-[var(--text-secondary)]">默认关闭。查找有数量与大小上限，跳过隐藏、构建和依赖目录；仍需安全设置中的目录递归读取授权，不能越过子路径的拒绝或单独审批规则。</p><label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={allowWrites} onChange={event => setAllowWrites(event.target.checked)} />允许在授权目录创建新文件（不覆盖已有文件）</label><p className="text-xs leading-relaxed text-[var(--text-secondary)]">复用同一任务对话读取历史、推进工作、提问和汇报。仅允许读取本地文件、待办和提问；文件创建继续经过现有权限与审批检查。不会使用账号、联网、执行命令或代发消息。按你配置的模型运行，可能产生模型费用。</p></>}
      {!isRobot && !isPersonal && <Select label="执行工作流" value={graphId} onChange={event => setGraphId(event.target.value)} options={[{ value: "", label: "选择已有可执行工作流" }, ...graphs.map(graph => ({ value: graph.id, label: graph.name }))]} />}
      <Select label="工作空间上下文" hint={isPersonal ? `保存后的授权目录：${scope.root}。目录范围由服务端校验并固定。` : "将名称和根路径提供给工作流；工具使用已保存的参数。"} value={workspaceId} onChange={event => { const next = event.target.value; setWorkspaceId(next); if (isPersonal && value && next !== workspaceId) { setAllowDiscovery(false); setAllowWrites(false); } }} options={[{ value: "", label: "选择工作空间" }, ...workspaces.filter(item => item.status === "active").map(item => ({ value: item.id, label: item.name }))]} />
      {isPersonal && scope.changed && <p role="status" className="text-xs leading-relaxed text-[var(--text-secondary)]">操作范围将改为上述目录。查找和新文件创建需重新勾选，安全读取／写入授权仍单独校验，不会自动取得新目录权限。</p>}
      {isPersonal && scope.workspaceRootChanged && <p role="status" className="text-xs leading-relaxed text-[var(--text-secondary)]">工作空间目录已变化，此委托仍固定使用原授权目录；启用计划或保存同一工作空间不会自动扩大范围。</p>}
      {!isRobot && !isPersonal && !graphs.length && <p className="text-xs text-[var(--text-secondary)]">先在工作流管理中创建可执行工作流。</p>}
      {!workspaces.some(item => item.status === "active") && <p className="text-xs text-[var(--text-secondary)]">先创建一个有效工作空间。</p>}
      <Select label="触发方式" value={kind} onChange={event => setKind(event.target.value as AutomationSchedule["kind"])} options={[{ value: "once", label: "仅一次" }, { value: "daily", label: "每天" }, { value: "weekly", label: "每周" }, ...(isPersonal ? [{ value: "file_changed", label: "本地文件内容变化" }, { value: "watch", label: "根据证据主动复查" }] : [])]} />
      {kind === "file_changed" && <Input label="监听文件绝对路径" value={filePath} onChange={event => setFilePath(event.target.value)} hint="授权目录内的现有普通文件，最多 1 MiB；每 10 秒检查内容。只唤醒本委托，不改变权限。" />}
      <Input label="IANA 时区" value={timezone} onChange={event => setTimezone(event.target.value)} list="automation-timezones" placeholder="Asia/Shanghai" />
      <datalist id="automation-timezones"><option value="Asia/Shanghai" /><option value="Asia/Tokyo" /><option value="Europe/London" /><option value="America/New_York" /><option value="UTC" /></datalist>
      {kind === "watch" ? <AttentionFields schedule={watch} onChange={setWatch} /> : kind === "file_changed" ? null : kind === "once" ? <Input label="执行日期与时间（所选时区）" type="datetime-local" value={localAt} onChange={event => setLocalAt(event.target.value)} /> : <Input label="执行时间（所选时区）" type="time" value={time} onChange={event => setTime(event.target.value)} />}
      {kind === "weekly" && <fieldset><legend className="text-xs font-medium text-[var(--text-secondary)] mb-2">执行日</legend><div className="flex flex-wrap gap-2">{weekdays.map((day, index) => <button type="button" key={day} aria-pressed={days.includes(index + 1)} onClick={() => setDays(current => current.includes(index + 1) ? current.filter(value => value !== index + 1) : [...current, index + 1].sort())} className={`rounded-lg border px-3 py-2 text-xs ${days.includes(index + 1) ? "border-[var(--accent-primary)] bg-[var(--accent-soft)] text-[var(--accent-primary)]" : "border-[var(--border-soft)]"}`}>周{day}</button>)}</div></fieldset>}
      <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={enabled} onChange={event => setEnabled(event.target.checked)} />{kind === "watch" ? "启用主动关注" : "启用自动执行"}</label>
      {kind !== "watch" && <p className="text-xs leading-relaxed text-[var(--text-secondary)]">{kind === "file_changed" ? "保存和重新启用时以当前文件为基线；暂停期间不追补。忙碌时合并最新变化，失败或中断后请显式运行恢复。变化记录与执行报告保存在同一任务。" : "应用运行时按计划触发；错过时间会跳过。夏令时不存在的重复时间跳过，重复时段只执行第一次。"}</p>}
      {isPersonal && <p className="text-xs leading-relaxed text-[var(--text-secondary)]">关闭弹窗、切换页面和刷新不取消已接收的工作。暂停计划只停止后续触发；“停止委托”同时暂停计划并停止当前轮次。失败、中断或等待确认时不自动重试；请打开任务对话处理。退出整个应用后不继续执行。</p>}
    </fieldset>
  </Modal>;
}
