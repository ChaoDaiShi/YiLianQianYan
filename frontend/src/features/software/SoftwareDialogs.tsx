import type {SoftwareCandidate, SoftwareCapabilityEvidence, SoftwareCapabilityEntry, SoftwareDimension, SoftwareOperation, SoftwareRegistration, SoftwareSession} from "../../api/software";
import {Badge, Button, EmptyState, Modal} from "../../components/ui";
import {formatSoftwareTime} from "./softwareForm";
import {softwareModeLabel, softwareSessionActive, softwareSessionLabel} from "./softwarePolicy";
export type SoftwareDialog = "create" | "edit" | "discover" | "launch" | "release" | "delete" | "history" | "details" | "review" | "review-launch" | null;
interface Props {
 dialog: SoftwareDialog; registration: SoftwareRegistration; session?: SoftwareSession;
 candidates: SoftwareCandidate[]; candidate: string; onCandidate: (hwnd: string) => void;
 history: SoftwareOperation[]; busy: boolean; error: string; reviewed: boolean;
 evidence?: SoftwareCapabilityEvidence;
 onReviewed: (value: boolean) => void; onClose: () => void; onConfirm: () => void;
 onReviewLaunch?: (operationId: string) => void; launchOperationId?: string; mutationBlocked?: boolean;
}
const titles = {discover: "选择已运行窗口", launch: "确认启动软件", release: "断开软件会话", delete: "移除软件登记", history: "软件操作记录", details: "软件会话详情", review: "核对未知结果", "review-launch": "核对启动结果"};
const dimensions: Record<SoftwareDimension, string> = {
 discover_launch: "发现与启动", display: "窗口显示", perceive: "控件感知", operate: "控件操作", background_run: "后台运行", background_interaction: "后台感知与操作",
};
const statusLabels: Record<SoftwareCapabilityEntry["status"], string> = {
 verified: "已验证", partial: "部分验证", unverified: "未验证", unavailable: "当前不可用", authorization_required: "尚未授权",
};
const evidenceLabels: Record<string, string> = {
 launch_receipt: "启动回执确认了目标窗口", attach_receipt: "已经发现并核对窗口身份", window_verified: "原生窗口已核对",
 window_hidden_or_minimized: "窗口当前隐藏或最小化", uia_observation: "当前窗口代次取得了控件观察", control_receipt: "当前窗口代次的控件操作已核实",
 window_process_alive: "窗口隐藏或最小化时仍核实到原进程", monitor_process_alive: "原窗口关闭后仍核实到原进程",
 background_control: "隐藏或最小化状态的控件动作已有回执", background_uia: "隐藏或最小化状态已有控件观察，操作待验证",
 windows_notification_only: "已接收应用级 Windows 通知，当前窗口的后台控件能力待验证", native_host_unavailable: "Windows 桌面宿主当前不可用",
};

function CapabilityEvidence({evidence}: {evidence?: SoftwareCapabilityEvidence}) {
 return <section aria-label="能力验证" className="space-y-2">
  <h3 className="font-medium">能力验证</h3>
  <dl className="software-facts">
   {(Object.keys(dimensions) as SoftwareDimension[]).map(dimension => {
    const row = evidence?.entries.find(item => item.dimension === dimension);
    const status = row?.status ?? "unverified";
    return <div key={dimension}>
     <dt>{dimensions[dimension]}</dt>
     <dd className="flex flex-wrap items-center gap-2">
      <Badge tone={status === "verified" ? "success" : status === "partial" ? "warning" : "default"}>{statusLabels[status]}</Badge>
      {row && evidenceLabels[row.source] && <span>{evidenceLabels[row.source]}</span>}
      {row?.observed_at != null && <time dateTime={new Date(row.observed_at).toISOString()}>{formatSoftwareTime(row.observed_at)}</time>}
     </dd>
    </div>;
   })}
  </dl>
 </section>;
}
export default function SoftwareDialogs(props: Props) {
 const {dialog, registration, session, busy, candidates, candidate, history} = props;
 if (!dialog || dialog === "create" || dialog === "edit") return null;
 const disabled = busy || ((dialog === "discover" || dialog === "launch") && props.mutationBlocked) || (dialog === "discover" && !candidates.some(item => item.identity.hwnd === candidate)) || ((dialog === "review" || dialog === "review-launch") && !props.reviewed);
 const confirmText = {discover: "连接所选窗口", launch: "确认启动此 EXE", release: "确认断开", delete: "确认移除登记", review: "已核对，保持停止", "review-launch": "已核对，不重复启动"};
 return <Modal open title={titles[dialog]} size={dialog === "discover" || dialog === "history" ? "xl" : "lg"} onClose={() => {if (!busy) props.onClose();}}
  footer={dialog === "history" || dialog === "details" ? <Button variant="secondary" onClick={props.onClose}>关闭</Button> : <><Button variant="secondary" disabled={busy} onClick={props.onClose}>取消</Button><Button disabled={disabled} loading={busy} onClick={props.onConfirm}>{confirmText[dialog]}</Button></>}>
  <div className="software-dialog-content">
   {dialog === "details" && <>
    <dl className="software-facts"><div><dt>软件</dt><dd>{registration.name} · {registration.adapter === "wechat" ? "Windows 微信适配" : registration.adapter === "generic" ? "通用 Windows 软件" : "受控测试窗口"}</dd></div><div><dt>程序路径</dt><dd>{registration.executable}</dd></div><div><dt>AI 许可</dt><dd>控件观察{registration.allow_observe ? "已允许" : "关闭"} · 窗口正文{registration.allow_content ? "已允许" : "关闭"} · 窗口文字识别{registration.allow_window_ocr ? "已允许" : "关闭"} · AI 操作{registration.allow_actions ? "可请求审批" : "关闭"}</dd></div>
     {session && <><div><dt>会话</dt><dd>{softwareSessionLabel(session.state)} · {softwareSessionActive(session) ? softwareModeLabel(session.window.mode) : "未连接"}</dd></div><div><dt>窗口身份</dt><dd>PID {session.window.identity.pid} · {session.window.identity.class_name} · HWND {session.window.identity.hwnd}</dd></div><div><dt>进程创建标识</dt><dd>{session.window.identity.process_started_at}</dd></div><div><dt>进程归属</dt><dd>{session.launched_by_host ? "由本应用启动；断开后继续运行" : "附着已有进程；仍由用户管理"}</dd></div><div><dt>实际状态</dt><dd>{softwareSessionActive(session) ? <>{session.window.visible ? "可见" : "不可见"} · {session.window.focused ? "前台" : "未聚焦"} · {session.window.minimized ? "已最小化" : "未最小化"}</> : "当前未连接，原窗口信息仅供核对"}</dd></div></>}
    </dl>
    <CapabilityEvidence evidence={props.evidence} />
    <p>{registration.adapter === "wechat" ? "连接不会登录或退出微信。" : "当前会话绑定所选程序窗口。"}观察、正文读取和控件操作使用登记时选择的许可。</p>
    <p className="software-notice">正常离开页面或退出时会尝试恢复独立窗口；强制结束宿主可能来不及恢复，请先解除嵌入再强制关闭。</p>
   </>}
   {props.mutationBlocked && dialog === "discover" && <p className="software-notice">可核对窗口候选；连接前请先在操作记录中核实原启动结果，不会自动重试。</p>}
   {dialog === "discover" && <>
    {candidates.length > 1 && <p>{registration.name}有多个可连接的窗口，请选择要显示的窗口。</p>}
    {candidates.length === 0 ? <EmptyState title="未找到匹配窗口" description="可确认启动已登记程序，或检查路径、进程权限和窗口是否仍存在。" /> : <div className="software-candidates" role="radiogroup" aria-label="运行窗口候选">
     {candidates.map((item, index) => <div key={item.identity.hwnd} className="software-candidate">
      <label><input type="radio" name="software-window" checked={candidate === item.identity.hwnd} onChange={() => props.onCandidate(item.identity.hwnd)} disabled={busy} />
       <strong>窗口 {index + 1}</strong><span>{item.minimized ? "已最小化" : item.visible ? "当前可见" : "当前隐藏"}</span>
      </label>
      <details><summary>窗口详情</summary>
       <p>PID {item.identity.pid} · {item.identity.class_name} · HWND {item.identity.hwnd}</p>
       <p className="software-path">{item.identity.executable}</p>
      </details>
     </div>)}
    </div>}</>}
   {dialog === "launch" && <><p>将直接启动以下已登记程序，不传入任意命令或登录参数。若出现多个窗口，请选择要连接的窗口。</p><dl className="software-facts"><div><dt>程序</dt><dd>{registration.executable}</dd></div><div><dt>参数</dt><dd>无</dd></div></dl><p>{registration.adapter === "wechat" ? "不会结束已有微信进程，也不会登录、退出或更改账号。" : "连接后可在会话详情中核对程序和窗口。"}</p></>}
   {dialog === "release" && <><p>将恢复窗口原有显示状态并解除这条会话。原进程继续运行，已执行的操作不会回滚。</p><p>PID {session?.window.identity.pid} · {session?.window.identity.class_name}</p></>}
   {dialog === "delete" && <p>将移除“{registration.name}”的登记配置。连接中的会话须先断开。</p>}
   {dialog === "history" && (history.length === 0 ? <EmptyState title="尚无操作记录" description="连接、显示切换与 AI 工具操作会保留可核对的执行结果。" /> : <ol className="software-history">{history.map(item => <li key={item.operation_id}><div><strong>{item.action}</strong><span>{softwareSessionLabel(item.state)}</span></div><time>{formatSoftwareTime(item.created_at)}</time><p>操作 ID：{item.operation_id}</p>{item.reason && <p>{item.reason}</p>}{item.action === "launch" && item.state === "result_unknown" && !item.session_id && props.onReviewLaunch && <Button size="sm" variant="secondary" onClick={() => props.onReviewLaunch!(item.operation_id)}>核对启动结果…</Button>}</li>)}</ol>)}
   {dialog === "review" && <><p>请亲自核对原窗口和原操作记录，确认实际结果后解除“待核实”屏障。此操作不会重放原动作，AI 仍保持停止；后续继续需另行恢复。</p><label className="software-review-check"><input type="checkbox" checked={props.reviewed} onChange={event => props.onReviewed(event.target.checked)} disabled={busy} /><span>我已核对实际窗口及原操作，不要求重试原动作</span></label></>}
   {dialog === "review-launch" && <><p>启动可能已经创建进程，但尚未取得确定的窗口结果。请检查任务栏和原操作记录；不会结束进程、再次启动或自动附着。</p><p className="software-path">{registration.executable}</p><p>原操作 ID：{props.launchOperationId}</p><label className="software-review-check"><input type="checkbox" checked={props.reviewed} onChange={event => props.onReviewed(event.target.checked)} disabled={busy} /><span>我已检查实际启动结果，确认解除待核实状态，不重复原启动</span></label></>}
   {props.error && <p role="alert" className="software-error">{props.error}</p>}
  </div>
 </Modal>;
}

