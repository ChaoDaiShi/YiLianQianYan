import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Modal } from "../../../components/ui";
import { requestResult } from "../../../core/api/http";
import { approvalModes, type ApprovalMode } from "../../../components/chat/chatRequestOptions";
import { goalNodeDestination } from "./GoalTrackingSummary";
import type { GoalTrackingData } from "./goalTracking";
import { goalActionPolicy } from "./goalActionPolicy";
import { goalRemindersMuted, setGoalRemindersMuted } from "./goalReminders";

export type GoalControlCommand = "task.cancel" | "task.pause" | "task.resume";
export default function GoalTaskControls({ data, error, busy, loading, onRefresh, onControl, canvas }: {
  data: GoalTrackingData; error?: string | null; busy: boolean; loading: boolean;
  onRefresh: (verify?: boolean) => Promise<void>; onControl: (command: GoalControlCommand) => Promise<void>;
  canvas?: { expert: boolean; openExpert: () => void };
}) {
  const policy = goalActionPolicy(data, error);
  const [open, setOpen] = useState(false);
  const [muted, setMuted] = useState(() => goalRemindersMuted(data.detail.graph_id));
  const [mode, setMode] = useState<ApprovalMode | null>(null);
  const [modeError, setModeError] = useState("");
  useEffect(() => {
    if (!open) return;
    let active = true; setMode(null); setModeError("");
    void requestResult<{ mode: ApprovalMode }>("GET", "/api/security/approval-mode").then(result => {
      if (!active) return;
      if (result.ok) setMode(result.data.mode); else setModeError(result.error);
    });
    return () => { active = false; };
  }, [open]);
  const opensCurrentCanvas = canvas && policy.next.href === goalNodeDestination(data.detail.graph_id);
  return <>
    {policy.next.kind === "navigate" && (opensCurrentCanvas
      ? !canvas.expert && <button type="button" className="goal-next-action" title={policy.next.reason} onClick={canvas.openExpert}>{policy.next.label}</button>
      : <Link className="goal-next-action" to={policy.next.href!} title={policy.next.reason}>{policy.next.label}</Link>)}
    <button type="button" disabled={busy || loading} onClick={() => void onRefresh()}>刷新状态</button>
    {policy.canVerify && <button type="button" disabled={busy || loading} onClick={() => void onRefresh(true)}>核验产物</button>}
    {policy.canPause && <button type="button" disabled={busy} onClick={() => void onControl("task.pause")}>暂停任务</button>}
    {data.detail.control_state === "paused" && <button type="button" disabled={busy || !policy.canResume} title={policy.blockedReason || undefined} onClick={() => void onControl("task.resume")}>恢复任务</button>}
    {policy.canCancel && <button type="button" disabled={busy} onClick={() => void onControl("task.cancel")}>取消任务</button>}
    <button type="button" onClick={() => setOpen(true)}>权限与提醒</button>
    {policy.blockedReason && (!canvas || !error) && <span className="goal-action-hint" role="status">{policy.blockedReason}</span>}
    <Modal open={open} onClose={() => setOpen(false)} title="权限与提醒" size="md">
      <div className="goal-policy-content">
        <p>{modeError ? `审批设置读取失败：${modeError}` : mode ? `当前审批模式：${approvalModes.find(item => item.value === mode)?.label || mode}` : "正在读取审批设置…"}</p>
        {mode && <p>{approvalModes.find(item => item.value === mode)?.description}</p>}
        <p>执行、重试和恢复都继续经过工具权限检查。需要审批时，先查看具体操作及风险，再由你决定；监听到状态变化不会自动批准操作。</p>
        <Link to="/settings?section=permissions" onClick={() => setOpen(false)}>查看权限设置</Link>
        <label><input type="checkbox" checked={!muted} onChange={event => { const next = !event.target.checked; setMuted(next); setGoalRemindersMuted(data.detail.graph_id, next); }} />本目标开启状态提醒</label>
        <p>仅在需要决定、失败、中断、需要重新验收或成果通过验收时显示应用内提醒。同一状态不会因轮询、切换入口而反复提示；首次打开历史记录保持安静。</p>
        <p>页面放到后台时保留最新提醒，返回应用后再显示。关闭提醒仍会保留任务状态和待办；取消任务会停止后续执行。</p>
      </div>
    </Modal>
  </>;
}
