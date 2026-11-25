import {afterEach, describe, expect, it, vi} from "vitest";
vi.mock("./controlSession", () => ({controlSessionHeaders: () => ({"x-yilian-control-session": "fixture"})}));
const native = vi.hoisted(() => ({invoke: vi.fn()}));
vi.mock("@tauri-apps/api/core", () => native);
import {acknowledgeSoftwareLaunch, attachSoftware, initializeSoftwareHost, launchSoftware, listSoftware, presentSoftware, SoftwareApiError, type SoftwareSession} from "./software";

const session: SoftwareSession = {
 id: "session/one", registration_id: "wechat", generation: 4, state: "active", launched_by_host: false, last_error: null,
 window: {identity: {hwnd: "123", pid: 9, process_started_at: "133577982328123456", executable: "C:/Apps/Weixin.exe", class_name: "FixtureWindow"}, mode: "independent", visible: true, focused: false, minimized: false, embedding_supported: false, reason: null},
};
const launchApproval = {id: "wechat", revision: 3, executable: "C:/Apps/Weixin.exe"};
afterEach(() => {vi.unstubAllGlobals(); native.invoke.mockReset();});
describe("software management API", () => {
 it("binds attach and presentation to exact identities without accepting a parent window", async () => {
  const fetch = vi.fn().mockImplementation(async () => new Response(JSON.stringify(session), {status: 200}));
  vi.stubGlobal("fetch", fetch);
  await attachSoftware("app/one", session.window.identity, "attach-op");
  await presentSoftware(session, "embedded", "present-op", {x: 10, y: 20, width: 500, height: 350});
  expect(new URL(fetch.mock.calls[0][0]).pathname).toBe("/api/software/registrations/app%2Fone/attach");
  expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({operation_id: "attach-op", identity: session.window.identity});
  expect(JSON.parse(fetch.mock.calls[1][1].body)).toEqual({generation: 4, operation_id: "present-op", mode: "embedded", viewport: {x: 10, y: 20, width: 500, height: 350}});
  expect(fetch.mock.calls[1][1].headers["x-yilian-control-session"]).toBe("fixture");
 });
 it("keeps uncertain launch responses distinguishable and never resends automatically", async () => {
  const fetch = vi.fn().mockRejectedValue(new Error("response lost after launch"));
  vi.stubGlobal("fetch", fetch);
  const error = await launchSoftware(launchApproval, "original-operation").catch(cause => cause);
  expect(error).toBeInstanceOf(SoftwareApiError);
  expect(error.uncertain).toBe(true);
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({operation_id: "original-operation", confirmed: true, revision: 3, expected_executable: "C:/Apps/Weixin.exe"});
 });
 it("rejects a malformed HTTP success instead of claiming an attached session", async () => {
  vi.stubGlobal("fetch", vi.fn().mockImplementation(async () => new Response("{}", {status: 200})));
  await expect(launchSoftware(launchApproval, "original-operation")).rejects.toMatchObject({code: "invalid_session_response", uncertain: true});
  await expect(listSoftware()).rejects.toMatchObject({code: "invalid_software_snapshot"});
 });
 it("retains actionable refusal text and its machine code", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({error: "stale_generation", message: "窗口身份已改变，请重新选择"}), {status: 409})));
  await expect(presentSoftware(session, "independent", "op")).rejects.toMatchObject({message: "窗口身份已改变，请重新选择", code: "stale_generation", uncertain: false});
 });
 it("acknowledges exactly the inspected launch without creating or replaying a launch", async () => {
  const fetch = vi.fn().mockImplementation(async () => new Response(JSON.stringify({acknowledged: true, replayed: false}), {status: 200}));
  vi.stubGlobal("fetch", fetch);
  await acknowledgeSoftwareLaunch("wechat", "original-unknown-launch");
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(new URL(fetch.mock.calls[0][0]).pathname).toBe("/api/software/registrations/wechat/acknowledge");
  expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({operation_id: "original-unknown-launch", confirmed: true});
 });
 it("keeps the original canonical executable in the launch confirmation request", async () => {
  const fetch = vi.fn().mockImplementation(async () => new Response(JSON.stringify(session), {status: 200}));
  vi.stubGlobal("fetch", fetch);
  const approved = {...launchApproval, executable: "\\\\?\\F:\\Apps\\Weixin.exe"};
  await launchSoftware(approved, "canonical-launch");
  expect(JSON.parse(fetch.mock.calls[0][1].body)).toMatchObject({revision: 3, expected_executable: "\\\\?\\F:\\Apps\\Weixin.exe"});
 });
 it("does not invoke native binding in SSR or ordinary browser mode", async () => {
  await initializeSoftwareHost();
  vi.stubGlobal("window", {});
  await initializeSoftwareHost();
  expect(native.invoke).not.toHaveBeenCalled();
 });
 it("binds the native host without an HWND argument, coalesces concurrent requests and reports failure", async () => {
  vi.stubGlobal("window", {__TAURI_INTERNALS__: {}});
  native.invoke.mockResolvedValue(undefined);
  await Promise.all([initializeSoftwareHost(), initializeSoftwareHost()]);
  expect(native.invoke.mock.calls).toEqual([["native_software_bind"]]);
  native.invoke.mockRejectedValue({message: "main window is unavailable"});
  await expect(initializeSoftwareHost()).rejects.toMatchObject({code: "native_software_bind_failed", message: "原生软件连接初始化失败：main window is unavailable"});
 });
});

