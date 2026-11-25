import type {SoftwareSession, SoftwareRegistration, SoftwareMonitorStatus} from "../../api/software";
import {Badge, Button} from "../../components/ui";
import {softwareModeLabel, softwareSessionActive, softwareSessionEmbedded, softwareSessionLabel, softwareSessionUnknown} from "./softwarePolicy";
interface Props {
 registration: SoftwareRegistration; session?: SoftwareSession; sessions: SoftwareSession[]; available: boolean; busy: boolean; uncertain?: boolean;
 onSession: (id: string) => void; onDiscover: () => void; onLaunch: () => void; onEdit: () => void;
 onHistory: () => void; onDelete: () => void; onDetails: () => void; onMode: (mode: "embedded" | "independent" | "focus") => void;
 onStop: () => void; onResume: () => void; onRelease: () => void; onReview: () => void;
 onReadChat?: () => void;
 monitor?: SoftwareMonitorStatus;
 onMonitorChange?: (enabled: boolean) => void;
}

const monitorState: Record<SoftwareMonitorStatus["state"], string> = {
 watching: "监管中",
 waiting_window: "等待新窗口",
 waiting_process: "等待软件重开",
 unavailable: "状态待核实",
};

const notificationState: Record<NonNullable<SoftwareMonitorStatus["notification_state"]>, string> = {
 checking: "正在检查系统通知",
 watching: "系统通知已接入",
 access_required: "系统通知等待 Windows 访问许可",
 unavailable: "系统通知来源暂不可用",
};
const contentState: Record<NonNullable<SoftwareMonitorStatus["content_state"]>, string> = {
 checking: "正在检查窗口内容",
 watching: "正在监控窗口内容和控件状态",
 permission_required: "窗口内容监管需要软件观察许可",
 unavailable: "当前窗口暂无可监控的内容或控件状态",
};
export default function SoftwareSessionPanel(props: Props) {
 const {registration, session, sessions, available, busy} = props;
 const active = session && softwareSessionActive(session);
 const unknown = session && softwareSessionUnknown(session);
 const embedded = softwareSessionEmbedded(session);
 const activeSessions = sessions.filter(softwareSessionActive);
 const monitoringOtherSession = Boolean(session && props.monitor?.session_id && props.monitor.session_id !== session.id);
 const monitor = monitoringOtherSession ? undefined : props.monitor;
 const notification = monitor?.notification_state;
 return <section className="software-session-panel" aria-label="软件会话与操作">
  <div className="software-session-toolbar">
   <div className="software-title-state">
    <h2 title={registration.name}>{registration.name}</h2>
    {session && <div className="software-session-state"><Badge tone={unknown ? "warning" : active ? "success" : "default"}>{softwareSessionLabel(session.state)}</Badge>{active && <Badge>{softwareModeLabel(session.window.mode)}</Badge>}</div>}
    <div className="software-permission-summary" aria-label="当前 AI 许可"><Badge tone={registration.allow_observe ? "warning" : "default"}>控件观察{registration.allow_observe ? "已允许" : "关闭"}</Badge><Badge tone={registration.allow_actions ? "warning" : "default"}>AI 操作{registration.allow_actions ? "可请求审批" : "关闭"}</Badge></div>
   </div>
   <div className="software-toolbar-meta">
    {(activeSessions.length > 1 || (!active && activeSessions.length > 0)) && <label className="software-session-picker">会话<select aria-label="已关联会话" value={session?.id ?? ""} disabled={busy} onChange={event => props.onSession(event.target.value)}>
     {!active && <option value={session?.id ?? ""}>选择已连接窗口</option>}
     {activeSessions.map((item, index) => <option key={item.id} value={item.id}>窗口 {index + 1} · {softwareModeLabel(item.window.mode)}</option>)}
    </select></label>}
    <Button size="sm" variant="ghost" disabled={busy} onClick={props.onDetails}>会话详情</Button>
    <Button size="sm" variant="ghost" disabled={busy} onClick={props.onEdit}>登记与许可</Button>
    <Button size="sm" variant="ghost" disabled={busy} onClick={props.onHistory}>操作记录</Button>
   </div>
  </div>
  <div className="software-row-actions">
   {!active && <Button size="sm" disabled={busy || !available} onClick={props.onDiscover}>{session ? "重新连接软件" : "连接软件"}</Button>}
   {!active && <><Button size="sm" variant="secondary" disabled={busy || !available || props.uncertain} onClick={props.onLaunch}>启动程序…</Button><Button size="sm" variant="ghost" disabled={busy || sessions.some(softwareSessionActive)} onClick={props.onDelete}>移除登记</Button></>}
   {active && <>
    {registration.adapter === "wechat" && props.onReadChat && <Button size="sm" variant="secondary" disabled={busy || !available || session.state !== "active" || !registration.allow_observe} title={registration.allow_observe ? "框选当前微信窗口里的群聊消息" : "请先在登记与许可中开启 AI 观察"} onClick={props.onReadChat}>读取当前群聊</Button>}
    {!embedded && <Button size="sm" disabled={busy || !available || !session.window.embedding_supported || unknown} onClick={() => props.onMode("embedded")}>嵌回忆涟千言</Button>}
    <Button size="sm" variant="secondary" disabled={busy || !available} onClick={() => props.onMode("focus")}>独立显示</Button>
    <Button size="sm" variant="secondary" disabled={busy || session.state === "stopped"} onClick={props.onStop}>停止 AI 操作</Button>
    {session.state === "stopped" && <Button size="sm" variant="secondary" disabled={busy || !available} onClick={props.onResume}>恢复 AI 会话</Button>}
    <Button size="sm" variant="ghost" disabled={busy} onClick={props.onRelease}>断开会话…</Button>
   </>}
   {props.onMonitorChange && <Button
    size="sm" variant="secondary" role="switch" aria-label="后台监管"
    aria-checked={Boolean(props.monitor)} title={monitoringOtherSession ? "正在监管此软件的其他会话，可点击停止" : "监管窗口、已许可观察的内容和此软件的 Windows 通知"}
    disabled={busy || (!props.monitor && (!available || !active || unknown))}
    onClick={() => props.onMonitorChange?.(!props.monitor)}
   >
    后台监管 · {monitoringOtherSession ? "其他会话" : monitor ? monitorState[monitor.state] : "关闭"}
   </Button>}
   {(notification === "access_required" || notification === "unavailable") && <span className="software-hint" role="status">
    {notificationState[notification]}
   </span>}
  </div>
  {monitor?.content_state && <span className="software-hint" role="status">
    {contentState[monitor.content_state]}
  </span>}
  {(notification === "checking" || notification === "watching") && <details className="software-hint software-monitor-details">
   <summary>系统通知详情</summary>
   <p role="status">{notificationState[notification]}</p>
  </details>}
  {monitor?.latest && <p className="software-hint" role="status">
   <strong>{monitor.latest.summary}。</strong>{" "}
   {monitor.latest.explanation
    ?? (monitor.latest.explaining ? "AI 正在解释…" : "已记录窗口事件，AI 解释暂不可用。")}
  </p>}
  {active && session.window.reason && <p className="software-notice">{session.window.reason}</p>}
  {session?.last_error && <p role="status" className="software-error">{session.last_error}</p>}
  {unknown && <div className="software-unknown"><p>有操作结果尚未确定。请检查原窗口和操作记录；系统不会自动重复原动作。</p><Button size="sm" variant="secondary" disabled={busy} onClick={props.onReview}>核对未知结果…</Button></div>}
 </section>;
}
