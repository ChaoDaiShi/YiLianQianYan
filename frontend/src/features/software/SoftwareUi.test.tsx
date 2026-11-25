import {describe, expect, it} from "vitest";
import {renderToStaticMarkup} from "react-dom/server";
import SoftwareSessionPanel from "./SoftwareSessionPanel";
import SoftwareRegistrationDialog from "./SoftwareRegistrationDialog";
import SoftwareDialogs from "./SoftwareDialogs";
import {emptySoftwareDraft, softwareRegistrationDraft} from "./softwareForm";
import type {SoftwareRegistration, SoftwareSession} from "../../api/software";
const registration: SoftwareRegistration = {id: "wechat", revision: 1, name: "微信", adapter: "wechat", executable: "C:/Apps/Weixin.exe", workspace_id: null, allow_observe: false, allow_actions: false};
const session: SoftwareSession = {id: "session", registration_id: "wechat", generation: 1, state: "active", launched_by_host: false, last_error: null, window: {identity: {hwnd: "12", pid: 100, process_started_at: "133577982328123456", executable: registration.executable, class_name: "WeChatFixture"}, mode: "independent", visible: true, focused: false, minimized: false, embedding_supported: false, reason: "此版本尚未验证嵌入，请保留独立窗口"}};
const noop = () => {};
const button = (html: string, label: string) => html.match(/<button\b[\s\S]*?<\/button>/g)?.find(markup => markup.includes(label)) ?? "";
const panel = (value: SoftwareSession, currentRegistration = registration) => renderToStaticMarkup(<SoftwareSessionPanel onReadChat={noop} registration={currentRegistration} session={value} sessions={[value]} available busy={false} onSession={noop} onDiscover={noop} onLaunch={noop} onEdit={noop} onHistory={noop} onDelete={noop} onDetails={noop} onMode={noop} onStop={noop} onResume={noop} onRelease={noop} onReview={noop} />);
describe("software workbench truthful states", () => {
 it("requires observation and a live WeChat session to read the visible chat", () => {
  const authorized = {...registration, allow_observe: true, allow_actions: false};
  expect(button(panel(session), "读取当前群聊")).toContain('disabled=""');
  const enabled = button(panel(session, authorized), "读取当前群聊");
  expect(enabled).not.toBe("");
  expect(enabled).not.toContain('disabled=""');
  expect(button(panel(session, {...authorized, adapter: "generic"}), "读取当前群聊")).toBe("");
 });
 it("keeps the embedded toolbar compact while preserving its primary window controls", () => {
  const html = panel({...session, window: {...session.window, mode: "embedded", embedding_supported: true, reason: null}});
  expect(button(html, "会话详情")).not.toBe("");
  expect(html).not.toContain(registration.executable);
  expect(html).not.toContain("WeChatFixture");
  expect(html).not.toContain("强制结束宿主可能来不及恢复");
  expect(button(html, "独立显示")).not.toBe("");
  expect(button(html, "停止 AI 操作")).not.toBe("");
  expect(button(html, "断开会话…")).not.toBe("");
 });
 it("keeps full window identity, executable and ownership available in the details dialog", () => {
  const html=renderToStaticMarkup(<SoftwareDialogs dialog="details" registration={registration} session={session} candidates={[]} candidate="" onCandidate={noop} history={[]} busy={false} error="" reviewed={false} onReviewed={noop} onClose={noop} onConfirm={noop} />);
  expect(html).toContain("软件会话详情");
  expect(html).toContain(registration.executable);
  expect(html).toContain("WeChatFixture");
  expect(html).toContain("附着已有进程；仍由用户管理");
  expect(html).toContain("连接不会登录或退出微信");
 });
 it("does not present cached embedded and visible flags as live after the original window is lost", () => {
  const html = panel({...session, state: "lost", last_error: "window_closed: The selected window no longer exists", window: {...session.window, mode: "embedded", visible: true, focused: true}});
  expect(html).toContain("原窗口已失效");
  expect(html).toContain("window_closed: The selected window no longer exists");
  expect(html).not.toContain("窗口不可用");
  expect(html).not.toContain("主窗嵌入");
  expect(html).not.toContain("可见</dd>");
  expect(html).not.toContain("可见 · 前台");
  expect(html).not.toContain("强制结束宿主可能来不及恢复");
  expect(button(html, "重新连接软件")).not.toContain('disabled=""');
  expect(button(html, "重新连接软件")).not.toBe("");
 });
 it("does not offer unverified embedding or imply ownership of attached processes", () => {
  const html = panel(session);
  expect(button(html, "嵌回忆涟千言")).toContain('disabled=""');
  expect(button(html, "会话详情")).not.toBe("");
  expect(html).toContain("此版本尚未验证嵌入");
  expect(html).toContain("控件观察关闭");
 });
 it("offers inspection of uncertain results without a replay button", () => {
  const html = panel({...session, state: "result_unknown"});
  expect(html).toContain("核对未知结果");
  expect(html).toContain("不会自动重复原动作");
  expect(html).not.toContain("重试原动作</button>");
 });
 it("starts registration permissions unchecked and explains the privacy boundary", () => {
  const html = renderToStaticMarkup(<SoftwareRegistrationDialog draft={emptySoftwareDraft()} workspaces={[]} active={false} busy={false} error="" onDraft={noop} onSave={noop} onClose={noop} />);
  expect(html).not.toContain('checked=""');
  expect(html).toContain('<option value="generic"');
  expect(html).toContain("观察可能包含当前界面的私密名称");
  expect(html).toContain("这项许可不替代 Gateway 审批");
 });
 it("never preselects one of multiple windows", () => {
  const candidates = [session.window, {...session.window, identity: {...session.window.identity, hwnd: "99", pid: 200}}];
  const html = renderToStaticMarkup(<SoftwareDialogs dialog="discover" registration={registration} candidates={candidates} candidate="" onCandidate={noop} history={[]} busy={false} error="" reviewed={false} onReviewed={noop} onClose={noop} onConfirm={noop} />);
  expect(html).not.toContain('checked=""');
  expect(html).toContain("PID 100");
  expect(html).toContain("PID 200");
  expect(button(html, "连接所选窗口")).toContain('disabled=""');
 });
 it("shows only registration removal and keeps the executable path visible before launch", () => {
  const html = renderToStaticMarkup(<SoftwareDialogs dialog="launch" registration={registration} candidates={[]} candidate="" onCandidate={noop} history={[]} busy={false} error="" reviewed={false} onReviewed={noop} onClose={noop} onConfirm={noop} />);
  expect(html).toContain(registration.executable);
  expect(html).toContain("不会结束已有微信进程");
  expect(html).toContain("确认启动此 EXE");
 });
 it("keeps a no-session unknown launch recoverable from history without offering a replay", () => {
  const html = renderToStaticMarkup(<SoftwareDialogs dialog="history" registration={registration} candidates={[]} candidate="" onCandidate={noop} history={[{operation_id: "unknown-launch", action: "launch", registration_id: "wechat", session_id: null, state: "result_unknown", reason: "进程可能已启动，需核对窗口", created_at: 10}]} busy={false} error="" reviewed={false} onReviewed={noop} onClose={noop} onConfirm={noop} onReviewLaunch={noop} />);
  expect(html).toContain("核对启动结果");
  expect(html).toContain("unknown-launch");
  expect(html).not.toContain("重试启动");
 });
 it("keeps save available when revoking permissions on an active canonical-path registration", () => {
  const current = {...registration, executable: "\\\\?\\F:\\Apps\\Weixin.exe", allow_observe: true, allow_actions: true};
  const draft = {...softwareRegistrationDraft(current), allow_observe: false, allow_actions: false};
  const html = renderToStaticMarkup(<SoftwareRegistrationDialog registration={current} draft={draft} workspaces={[]} active busy={false} error="" onDraft={noop} onSave={noop} onClose={noop} />);
  expect(html).toContain('value="F:\\Apps\\Weixin.exe"');
  expect(button(html, "保存登记")).not.toContain('disabled=""');
  expect(html).not.toContain("请填写本机现有 EXE");
 });
});

