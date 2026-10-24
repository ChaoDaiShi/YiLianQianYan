import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { CalendarClock, Play, Plus, RefreshCw, Save, Square, WandSparkles } from "lucide-react";
import { Link } from "react-router-dom";
import Button from "../../components/ui/Button";
import { listWorkspaces, type Workspace } from "../../api/workspaces";
import {
  createRobotAutomation, createRobotMission, getRobotPolicy, getRobotRun, listRobotAutomations,
  listRobotMissions, listRobotRuns, listRobotSends, setRobotAutomationEnabled, startRobotMission, stopRobotRun,
  updateRobotMission, type RobotAction, type RobotAutomation, type RobotAutomationSchedule,
  type RobotMission, type RobotMissionInput, type RobotPolicy, type RobotRun, type RobotRunDetails,
} from "../../api/channelRobot";
import type { ChannelPreset, ExternalChannel } from "../../api/channels";

const problem = (value: unknown) => value instanceof Error ? value.message : "机器人操作失败";
const runLabels: Record<string, string> = {
  queued: "排队中", running: "运行中", awaiting_approval: "等待审阅",
  completed: "已完成", stopped: "已停止", failed: "失败",
};
const actionLabels: Record<string, string> = {
  running: "执行中", sent: "平台已确认受理", succeeded: "已完成", send_failed: "发送失败",
  delivery_unknown: "送达状态未知", failed: "失败",
};
const weekdayNames = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
interface MissionDraftState { name: string; goal: string; preset_id: string | null; workspace_id: string | null }
const missionDrafts = new Map<string, MissionDraftState>();
const selectedMissions = new Map<string, string>();

function missionScopeKey(channel: ExternalChannel) {
  return channel.id + "\u0000" + (channel.self_id ?? "");
}
function missionDraftKey(channel: ExternalChannel, missionId: string | null) {
  return missionScopeKey(channel) + "\u0000" + (missionId ?? "__new__");
}
function missionDraftFromMission(mission: RobotMission): MissionDraftState {
  return { name: mission.name, goal: mission.goal, preset_id: mission.preset_id, workspace_id: mission.workspace_id };
}
function emptyMissionDraft(): MissionDraftState {
  return { name: "QQ 目标任务", goal: "", preset_id: null, workspace_id: null };
}

function actionTarget(action: RobotAction) {
  if (action.peer_type === "account") return "QQ 账号";
  if (action.peer_type === "friends") return "好友列表";
  if (action.peer_type === "groups") return "群列表";
  if (action.peer_type === "recent") return "最近会话";
  if (action.peer_type === "message") return action.peer_id ? "消息 " + action.peer_id : "单条消息";
  if (action.peer_type === "group") return action.peer_id ? "群 " + action.peer_id : "QQ 群";
  if (action.peer_type === "private") return action.peer_id ? "好友 " + action.peer_id : "QQ 好友";
  return "QQ 工具操作";
}

function actionBody(action: RobotAction) {
  if (typeof action.arguments === "object" && action.arguments !== null && !Array.isArray(action.arguments)) {
    const args = action.arguments as Record<string, unknown>;
    const message = args.message;
    if (typeof message === "string") return message;
    if (Array.isArray(message)) return message.flatMap(part => {
      if (typeof part !== "object" || part === null || Array.isArray(part)) return [];
      const segment = part as Record<string, unknown>;
      if (segment.type !== "text" || typeof segment.data !== "object" || segment.data === null || Array.isArray(segment.data)) return [];
      const text = (segment.data as Record<string, unknown>).text;
      return typeof text === "string" ? [text] : [];
    }).join("");
    if (typeof args.text === "string") return args.text;
  }
  return "";
}

export default function ChannelRobotPanel({ channel, presets, initialRunId = "", policyRevision = 0 }: { channel: ExternalChannel; presets: ChannelPreset[]; initialRunId?: string; policyRevision?: number }) {
  const [policy, setPolicy] = useState<RobotPolicy | null>(null);
  const [missions, setMissions] = useState<RobotMission[]>([]);
  const [runs, setRuns] = useState<RobotRun[]>([]);
  const [manualSends, setManualSends] = useState<RobotAction[]>([]);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [automations, setAutomations] = useState<RobotAutomation[]>([]);
  const [selectedMissionId, setSelectedMissionId] = useState("");
  const [name, setName] = useState("QQ 目标任务");
  const [goal, setGoal] = useState("");
  const [presetId, setPresetId] = useState("");
  const [missionWorkspace, setMissionWorkspace] = useState("");
  const [selectedRunId, setSelectedRunId] = useState(initialRunId);
  const [details, setDetails] = useState<RobotRunDetails | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [running, setRunning] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [scheduleOpen, setScheduleOpen] = useState(false);
  const [scheduleName, setScheduleName] = useState("");
  const [scheduleKind, setScheduleKind] = useState<RobotAutomationSchedule["kind"]>("weekly");
  const [scheduleTime, setScheduleTime] = useState("09:30");
  const [scheduleAt, setScheduleAt] = useState("");
  const [weekdays, setWeekdays] = useState<number[]>([1]);
  const [scheduleWorkspace, setScheduleWorkspace] = useState("");
  const [scheduleBusy, setScheduleBusy] = useState(false);
  const [automationBusyId, setAutomationBusyId] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const missionDraftRef = useRef<MissionDraftState>(emptyMissionDraft());
  const scopeKey = missionScopeKey(channel);

  const selectedMission = missions.find(item => item.id === selectedMissionId) ?? null;
  const selectedRun = runs.find(item => item.id === selectedRunId) ?? details?.run ?? null;
  const missionDirty = !selectedMission || selectedMission.name !== name.trim() || selectedMission.goal !== goal.trim() || selectedMission.preset_id !== (presetId || null) || selectedMission.workspace_id !== (missionWorkspace || null);
  const policyMatchesAccount = Boolean(channel.self_id && policy?.account_id && policy.account_id === channel.self_id);
  const activeWorkspaces = useMemo(() => workspaces.filter(item => item.status === "active"), [workspaces]);
  const missionAutomations = useMemo(
    () => automations.filter(item => {
      const target = item.target;
      return target?.kind === "agent_turn" && missions.some(mission => mission.id === target.mission_id);
    }),
    [automations, missions],
  );

  const loadRuns = useCallback(async () => {
    const [runResult, automationResult, sendResult] = await Promise.all([listRobotRuns(channel.id), listRobotAutomations(), listRobotSends(channel.id)]);
    setRuns(runResult.runs);
    setAutomations(automationResult.automations);
    setManualSends(sendResult.actions);
  }, [channel.id]);

  useEffect(() => {
    let active = true;
    void getRobotPolicy(channel.id).then(result => { if (active) setPolicy(result.policy); })
      .catch(reason => { if (active) setError(problem(reason)); });
    return () => { active = false; };
  }, [channel.id, policyRevision]);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError("");
    void Promise.all([listRobotMissions(channel.id), listRobotRuns(channel.id), listWorkspaces(), listRobotAutomations(), listRobotSends(channel.id)])
      .then(([missionResult, runResult, workspaceResult, automationResult, sendResult]) => {
        if (!active) return;
        setMissions(missionResult.missions);
        setRuns(runResult.runs);
        setManualSends(sendResult.actions);
        setWorkspaces(workspaceResult.ok ? workspaceResult.data : []);
        setAutomations(automationResult.automations);
        const hasSavedSelection = selectedMissions.has(scopeKey);
        const savedSelection = selectedMissions.get(scopeKey) ?? "";
        const chosenMission = hasSavedSelection && savedSelection
          ? missionResult.missions.find(item => item.id === savedSelection) ?? missionResult.missions[0] ?? null
          : hasSavedSelection ? null : missionResult.missions[0] ?? null;
        if (chosenMission) {
          selectedMissions.set(scopeKey, chosenMission.id);
          setSelectedMissionId(chosenMission.id);
          applyMissionDraft(chosenMission.id, missionDraftFromMission(chosenMission));
        } else {
          selectedMissions.set(scopeKey, "");
          setSelectedMissionId("");
          applyMissionDraft(null, emptyMissionDraft());
        }
        setSelectedRunId(initialRunId && runResult.runs.some(item => item.id === initialRunId) ? initialRunId : runResult.runs[0]?.id ?? "");
        setError(workspaceResult.ok ? "" : workspaceResult.error);
      })
      .catch(reason => { if (active) setError(problem(reason)); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [channel.id, channel.self_id, initialRunId, scopeKey]);

  useEffect(() => {
    if (!selectedMission) return;
    applyMissionDraft(selectedMission.id, missionDraftFromMission(selectedMission));
  }, [selectedMission?.id, selectedMission?.updated_at]);

  function applyMissionDraft(missionId: string | null, base: MissionDraftState) {
    const draft = missionDrafts.get(missionDraftKey(channel, missionId)) ?? base;
    missionDraftRef.current = draft;
    setName(draft.name);
    setGoal(draft.goal);
    setPresetId(draft.preset_id ?? "");
    setMissionWorkspace(draft.workspace_id ?? "");
  }

  function updateMissionDraft(patch: Partial<MissionDraftState>) {
    const next = { ...missionDraftRef.current, ...patch };
    missionDraftRef.current = next;
    missionDrafts.set(missionDraftKey(channel, selectedMissionId || null), next);
    setName(next.name);
    setGoal(next.goal);
    setPresetId(next.preset_id ?? "");
    setMissionWorkspace(next.workspace_id ?? "");
  }

  function startNewMissionDraft() {
    selectedMissions.set(scopeKey, "");
    setSelectedMissionId("");
    applyMissionDraft(null, emptyMissionDraft());
    setNotice("");
  }

  useEffect(() => {
    if (!selectedRunId) { setDetails(null); return; }
    let active = true;
    let inFlight = false;
    const refresh = async () => {
      if (!active || inFlight || document.visibilityState !== "visible") return;
      inFlight = true;
      try {
        const result = await getRobotRun(channel.id, selectedRunId);
        if (active) setDetails(result);
      } catch (reason) {
        if (active) setError(problem(reason));
      } finally { inFlight = false; }
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), 3000);
    return () => { active = false; window.clearInterval(timer); };
  }, [channel.id, selectedRunId]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void loadRuns().catch(reason => setError(problem(reason)));
    }, 4000);
    return () => window.clearInterval(timer);
  }, [loadRuns]);

  async function saveMission() {
    if (!goal.trim() || !name.trim() || saving) return;
    setSaving(true);
    setError("");
    setNotice("");
    const input: RobotMissionInput = { name: name.trim(), goal: goal.trim(), preset_id: presetId || null, workspace_id: missionWorkspace || null };
    const draftKey = missionDraftKey(channel, selectedMissionId || null);
    try {
      const result = selectedMission
        ? await updateRobotMission(channel.id, selectedMission.id, input)
        : await createRobotMission(channel.id, input);
      setMissions(current => selectedMission
        ? current.map(item => item.id === result.mission.id ? result.mission : item)
        : [result.mission, ...current]);
      missionDrafts.delete(draftKey);
      missionDrafts.delete(missionDraftKey(channel, result.mission.id));
      selectedMissions.set(scopeKey, result.mission.id);
      setSelectedMissionId(result.mission.id);
      applyMissionDraft(result.mission.id, missionDraftFromMission(result.mission));
      setNotice("目标已保存");
    } catch (reason) { setError(problem(reason)); }
    finally { setSaving(false); }
  }

  async function startRun() {
    if (!selectedMission || running || !policy?.enabled || !policyMatchesAccount || !channel.enabled) return;
    setRunning(true);
    setError("");
    setNotice("");
    try {
      const result = await startRobotMission(channel.id, selectedMission.id);
      setRuns(current => [result.run, ...current.filter(item => item.id !== result.run.id)]);
      setSelectedRunId(result.run.id);
      setNotice("目标已提交，正在读取运行状态");
    } catch (reason) { setError(problem(reason)); }
    finally { setRunning(false); }
  }

  async function stopRun() {
    if (!selectedRun || stopping) return;
    setStopping(true);
    setError("");
    try {
      const result = await stopRobotRun(channel.id, selectedRun.id);
      setRuns(current => current.map(item => item.id === result.run.id ? result.run : item));
      setDetails(current => current ? { ...current, run: result.run } : current);
      setNotice("已请求停止运行");
    } catch (reason) { setError(problem(reason)); }
    finally { setStopping(false); }
  }

  function selectMission(mission: RobotMission) {
    selectedMissions.set(scopeKey, mission.id);
    setSelectedMissionId(mission.id);
    applyMissionDraft(mission.id, missionDraftFromMission(mission));
    setNotice("");
  }

  async function saveSchedule(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedMission || !scheduleName.trim() || !scheduleWorkspace || scheduleBusy) return;
    setScheduleBusy(true);
    setError("");
    setNotice("");
    const schedule: RobotAutomationSchedule = scheduleKind === "once"
      ? { kind: "once", local_at: scheduleAt }
      : scheduleKind === "daily"
        ? { kind: "daily", time: scheduleTime }
        : { kind: "weekly", time: scheduleTime, weekdays: [...weekdays].sort((a, b) => a - b) };
    try {
      const result = await createRobotAutomation({
        expected_revision: null,
        name: scheduleName.trim(),
        workflow_graph_id: "",
        target: { kind: "agent_turn", mission_id: selectedMission.id },
        workspace_id: scheduleWorkspace,
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        schedule,
        enabled: true,
      });
      setAutomations(current => [result.automation, ...current.filter(item => item.id !== result.automation.id)]);
      setScheduleOpen(false);
      setNotice("计划已保存到自动化调度");
    } catch (reason) { setError(problem(reason)); }
    finally { setScheduleBusy(false); }
  }

  async function toggleAutomation(item: RobotAutomation) {
    setAutomationBusyId(item.id);
    setError("");
    try {
      const result = await setRobotAutomationEnabled(item.id, !item.enabled, item.revision);
      setAutomations(current => current.map(entry => entry.id === item.id ? result.automation : entry));
    } catch (reason) { setError(problem(reason)); }
    finally { setAutomationBusyId(""); }
  }

  if (loading) return <div className="channel-workspace-state" role="status">正在读取目标与运行记录…</div>;
  if (channel.provider !== "onebot11") return <section className="channel-robot-unavailable" aria-label="机器人自主能力说明"><h2>此渠道没有 QQ 目标操作能力</h2><p>{channel.provider === "wechat_onebot" ? "Akasha-WeChat 提供独立的文本收发接入；QQ 通讯录、历史和目标任务工具不会移用到此微信渠道。" : "机器人目标操作目前仅连接到 OneBot 11 QQ 渠道。"}</p></section>;

  return <section className="channel-robot-panel" aria-label="QQ 机器人目标任务">
    <div className="channel-robot-header"><div><h2>机器人目标</h2><p>从桌面启动已授权的 QQ 多步任务；运行步骤与平台动作来自本机记录。</p></div><Button variant="ghost" size="sm" onClick={() => void loadRuns().catch(reason => setError(problem(reason)))}><RefreshCw size={14} />刷新运行</Button></div>
    {(!policy?.enabled || !policyMatchesAccount) && <p className="channel-robot-permission-note" role="status">请先在“设置”中启用 QQ 自主模式、填写主人 ID，并确认权限范围绑定当前账号。向外发送仍遵循已保存的对象范围。</p>}
    {error && <p className="channels-error" role="alert">{error}</p>}
    {notice && <p className="channel-robot-notice" role="status">{notice}</p>}
    <div className="channel-robot-layout">
      <aside className="channel-mission-list" aria-label="已保存目标">
        <div className="channel-mission-list-heading"><div><h3>已保存目标</h3><span>{missions.length} 个目标</span></div><Button variant="ghost" size="sm" aria-label="新建目标" onClick={startNewMissionDraft}><Plus size={15} /></Button></div>
        {missions.map(item => <button key={item.id} type="button" className={selectedMissionId === item.id ? "selected" : ""} aria-pressed={selectedMissionId === item.id} onClick={() => selectMission(item)}>
          <strong>{item.name}</strong><span>{item.goal}</span><small>{new Date(item.updated_at).toLocaleString()}</small>
        </button>)}
        {!missions.length && <p className="channel-workspace-muted">还没有保存目标。</p>}
        <div className="channel-run-list-heading"><h3>最近运行</h3><span>{runs.length} 次</span></div>
        <div className="channel-run-list" aria-label="机器人运行记录">
          {runs.map(item => <button key={item.id} type="button" className={selectedRunId === item.id ? "selected" : ""} aria-pressed={selectedRunId === item.id} onClick={() => setSelectedRunId(item.id)}>
            <span><strong>{runLabels[item.status] ?? item.status}</strong><time>{new Date(item.updated_at).toLocaleTimeString()}</time></span><small>{item.goal}</small>
          </button>)}
          {!runs.length && <p className="channel-workspace-muted">暂无运行记录。</p>}
        </div>
      </aside>

      <div className="channel-robot-detail">
        <section className="channel-mission-editor" aria-label="目标编辑器">
          <div className="channel-section-heading"><span><WandSparkles size={13} /></span><div><h3>{selectedMission ? "目标内容" : "新建目标"}</h3><p>保存后可立即运行，也可创建周期计划。</p></div></div>
          <label>任务名称<input aria-label="任务名称" maxLength={80} value={name} onChange={event => updateMissionDraft({ name: event.target.value })} /></label>
          <label>目标任务<textarea aria-label="目标任务" rows={4} maxLength={16000} value={goal} onChange={event => updateMissionDraft({ goal: event.target.value })} placeholder="例如：查看最近会话，阅读活动小组的讨论并给我摘要。" /></label>
          <div className="channel-robot-select-row">
            <label>执行预设<select aria-label="执行预设" value={presetId} onChange={event => updateMissionDraft({ preset_id: event.target.value || null })}><option value="">使用渠道预设</option>{presets.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
            <label>工作空间<select aria-label="任务工作空间" value={missionWorkspace} onChange={event => updateMissionDraft({ workspace_id: event.target.value || null })}><option value="">不绑定工作空间</option>{activeWorkspaces.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
          </div>
          <div className="channel-mission-actions"><Button variant="secondary" disabled={saving || !name.trim() || !goal.trim()} loading={saving} onClick={() => void saveMission()}><Save size={14} />保存目标</Button><Button disabled={!selectedMission || missionDirty || !channel.enabled || !policy?.enabled || !policyMatchesAccount || running} loading={running} loadingLabel="提交中…" onClick={() => void startRun()}><Play size={14} />开始运行</Button><Button variant="secondary" disabled={!selectedMission || missionDirty} onClick={() => { setScheduleName(selectedMission?.name ?? ""); setScheduleWorkspace(selectedMission?.workspace_id ?? activeWorkspaces[0]?.id ?? ""); setScheduleOpen(current => !current); }}><CalendarClock size={14} />计划任务</Button></div>
          {selectedMission && missionDirty && <p className="channel-workspace-muted" role="status">目标内容有未保存的修改，保存后才能运行或创建计划。</p>}
          {scheduleOpen && selectedMission && <form className="channel-schedule-form" onSubmit={event => void saveSchedule(event)}>
            <div><h3>为目标安排执行时间</h3><p>计划在忆涟千言运行时触发；错过的计划时点不会补跑。</p></div>
            <label>计划名称<input aria-label="计划名称" maxLength={80} required value={scheduleName} onChange={event => setScheduleName(event.target.value)} /></label>
            <label>执行频率<select aria-label="执行频率" value={scheduleKind} onChange={event => setScheduleKind(event.target.value as RobotAutomationSchedule["kind"])}><option value="once">一次</option><option value="daily">每天</option><option value="weekly">每周</option></select></label>
            {scheduleKind === "once" ? <label>执行时间<input aria-label="执行时间" required type="datetime-local" value={scheduleAt} onChange={event => setScheduleAt(event.target.value)} /></label> : <label>执行时间<input aria-label="执行时间" required type="time" value={scheduleTime} onChange={event => setScheduleTime(event.target.value)} /></label>}
            {scheduleKind === "weekly" && <fieldset className="channel-weekdays"><legend>每周日期</legend>{weekdayNames.map((day, index) => <label key={day}><input type="checkbox" aria-label={day} checked={weekdays.includes(index + 1)} onChange={event => setWeekdays(current => event.target.checked ? [...current, index + 1].sort((a, b) => a - b) : current.filter(item => item !== index + 1))} />{day}</label>)}</fieldset>}
            <label>工作空间<select aria-label="工作空间" required value={scheduleWorkspace} onChange={event => setScheduleWorkspace(event.target.value)}><option value="">选择工作空间</option>{activeWorkspaces.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
            {!activeWorkspaces.length && <p className="channels-error">没有可用于 Automation 的工作空间。</p>}
            <div className="channel-mission-actions"><Button type="button" variant="ghost" onClick={() => setScheduleOpen(false)}>取消</Button><Button type="submit" disabled={scheduleBusy || !scheduleName.trim() || !scheduleWorkspace || (scheduleKind === "weekly" && weekdays.length === 0)} loading={scheduleBusy}><CalendarClock size={14} />保存计划任务</Button></div>
          </form>}
        </section>

        <section className="channel-run-detail" aria-label="运行详情">
          <div className="channel-run-detail-heading"><div><h3>执行步骤</h3><p>{selectedRun ? (selectedRun.origin === "owner" ? "QQ 主人" : selectedRun.origin === "automation" ? "计划触发" : "桌面触发") + " · " + (runLabels[selectedRun.status] ?? selectedRun.status) : "等待运行"}</p></div>{selectedRun && ["queued", "running", "awaiting_approval"].includes(selectedRun.status) && <Button variant="secondary" size="sm" disabled={stopping} loading={stopping} onClick={() => void stopRun()}><Square size={13} />停止运行</Button>}</div>
          {selectedRun ? <>
            <p className="channel-run-goal">{selectedRun.goal}</p>
            {selectedRun.error && <p className="channels-error" role="alert">{selectedRun.error}</p>}
            {selectedRun.status === "awaiting_approval" && <p className="channel-robot-approval-link"><Link to={"/chat/" + encodeURIComponent(selectedRun.conversation_id)}>查看并审阅本次工具批准</Link></p>}
            {selectedRun.answer && <div className="channel-run-answer"><span>任务报告</span><p>{selectedRun.answer}</p></div>}
            {details && details.actions.length > 0 ? <ol className="channel-run-actions">{details.actions.map(action => <li key={action.id} data-status={action.status}>
              <div><strong>{action.tool_name}</strong><span>{actionTarget(action)}</span><small>{actionLabels[action.status] ?? action.status}</small></div>
              {action.error && <p>{action.error}</p>}
              {action.platform_message_id && <p>平台回执：{action.platform_message_id}</p>}
              {action.result !== null && <pre>{JSON.stringify(action.result, null, 2)}</pre>}
            </li>)}</ol> : <p className="channel-workspace-muted">此运行尚无已记录步骤。</p>}
          </> : <div className="channel-workspace-empty"><span className="channel-workspace-empty-icon"><Play size={18} /></span><h3>运行记录会显示在这里</h3><p>开始任务后，展示实际工具动作、对象、返回结果与发送回执。</p></div>}
        </section>
      </div>
    </div>

    <section className="channel-scheduled-missions" aria-label="已保存计划任务"><div><h3>计划任务</h3><span>保存至现有 Automation</span></div>{missionAutomations.map(item => <article key={item.id}>
      <span><strong>{item.name}</strong><small>{item.schedule.kind === "weekly" ? "每周 " + item.schedule.weekdays.map(day => weekdayNames[day - 1]).join("、") + " " + item.schedule.time : item.schedule.kind === "daily" ? "每天 " + item.schedule.time : "一次 · " + item.schedule.local_at} · {item.timezone}</small><small>下次执行：{item.next_run_at ? new Date(item.next_run_at).toLocaleString() : item.enabled ? "无下一次执行" : "已停用"}</small></span>
      <Button variant="ghost" size="sm" disabled={automationBusyId === item.id} loading={automationBusyId === item.id} onClick={() => void toggleAutomation(item)}>{item.enabled ? "停用计划" : "启用计划"}</Button>
    </article>)}{missionAutomations.length === 0 && <p className="channel-workspace-muted">此渠道还没有保存的机器人计划。</p>}</section>
    <section className="channel-manual-send-history" aria-label="手动发送记录"><div><h3>手动发送记录</h3><span>最近 {manualSends.length} 条</span></div>{manualSends.map(action => <article key={action.id} data-status={action.status}>
      <div><strong>{actionTarget(action)}</strong><span>账号 {action.account_id} · {actionLabels[action.status] ?? action.status}</span></div>
      {actionBody(action) && <p className="channel-manual-send-body">{actionBody(action)}</p>}
      {action.platform_message_id && <small>平台回执：{action.platform_message_id}</small>}
      {action.error && <p className="channels-error">{action.error}</p>}
    </article>)}{manualSends.length === 0 && <p className="channel-workspace-muted">暂无手动发送记录。</p>}</section>
  </section>;
}
