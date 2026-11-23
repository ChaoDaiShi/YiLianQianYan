import type {SoftwareSession, SoftwareRegistration, SoftwareOperation} from "../../api/software";
const ended = new Set(["released", "interrupted", "closed", "exited", "lost"]);
export const softwareSessionActive = (session: SoftwareSession) => !ended.has(session.state);
export function softwareSessionEmbedded(session: SoftwareSession | null | undefined): boolean {
 return session != null && softwareSessionActive(session) && session.window.mode === "embedded";
}
export const softwareSessionUnknown = (session: SoftwareSession) => session.state === "result_unknown" || session.state === "unknown";
export const softwareSessionLabel = (state: string) => ({
 active: "已连接", stopped: "AI 已停止", released: "已断开", interrupted: "应用重启后中断",
 result_unknown: "结果待核实", unknown: "结果待核实", exited: "进程已退出", lost: "原窗口已失效", error: "连接异常",
 verified: "核验通过", running: "执行中", failed: "执行失败", cancelled: "已取消", acknowledged_unknown: "已核对（未重放）",
}[state] ?? state);
export function filterSoftware(records: SoftwareRegistration[], query: string, workspace: string) {
 const word = query.trim().toLocaleLowerCase();
 return records.filter(item => (!workspace || item.workspace_id === workspace) && (item.name + " " + item.executable).toLocaleLowerCase().includes(word));
}
export const softwareHistory = (records: SoftwareOperation[], registrationId: string) => records.filter(item => item.registration_id === registrationId).slice().reverse();
export function softwareModeLabel(mode: string) {
 return mode === "embedded" ? "主窗嵌入" : mode === "independent" || mode === "detached" ? "独立窗口" : mode;
}
export function clampSoftwareViewport(rect: Pick<DOMRect, "x" | "y" | "width" | "height">) {
 if (![rect.x, rect.y, rect.width, rect.height].every(Number.isFinite) || rect.width < 64 || rect.height < 64) return null;
 return {x: Math.max(0, Math.round(rect.x)), y: Math.max(0, Math.round(rect.y)), width: Math.min(8192, Math.round(rect.width)), height: Math.min(8192, Math.round(rect.height))};
}
export function softwareVisibleViewport(rect: Pick<DOMRect, "x" | "y" | "width" | "height">, clip: Pick<DOMRect, "x" | "y" | "width" | "height">) {
 const left = Math.max(0, rect.x, clip.x), top = Math.max(0, rect.y, clip.y);
 const right = Math.min(rect.x + rect.width, clip.x + clip.width), bottom = Math.min(rect.y + rect.height, clip.y + clip.height);
 return clampSoftwareViewport({x: left, y: top, width: right - left, height: bottom - top});
}
