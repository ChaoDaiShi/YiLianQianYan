import {describe, expect, it} from "vitest";
import {clampSoftwareViewport, softwareVisibleViewport, filterSoftware, softwareSessionActive, softwareSessionUnknown} from "./softwarePolicy";
import type {SoftwareRegistration, SoftwareSession} from "../../api/software";
const software = (id: string, workspace_id: string): SoftwareRegistration => ({id, workspace_id, revision: 1, name: "微信", executable: "C:/Apps/Weixin.exe", adapter: "wechat", allow_observe: false, allow_actions: false});
describe("software identity and presentation policy", () => {
 it("keeps same executable registrations separated by project", () => {
  const a = software("a", "project-a"), b = software("b", "project-b");
  expect(filterSoftware([a, b], "weixin.exe", "project-a")).toEqual([a]);
  expect(filterSoftware([a, b], "missing", "")).toEqual([]);
 });
 it.each(["released", "lost", "interrupted"])("does not offer continuation of stale %s sessions", state => {
  expect(softwareSessionActive({state} as SoftwareSession)).toBe(false);
 });
 it("keeps an unknown result attached but explicitly blocked for review", () => {
  const value = {state: "result_unknown"} as SoftwareSession;
  expect(softwareSessionActive(value)).toBe(true);
  expect(softwareSessionUnknown(value)).toBe(true);
 });
 it("rejects collapsed or nonfinite geometry instead of moving to a guessed region", () => {
  expect(clampSoftwareViewport({x: 1, y: 1, width: 20, height: 600})).toBeNull();
  expect(clampSoftwareViewport({x: NaN, y: 0, width: 600, height: 600})).toBeNull();
  expect(clampSoftwareViewport({x: -10, y: 72.6, width: 700.2, height: 380.4})).toEqual({x: 0, y: 73, width: 700, height: 380});
 });
 it("keeps native bounds inside the visible scrolling pane and rejects a fully hidden viewport", () => {
  expect(softwareVisibleViewport({x: 240, y: -20, width: 700, height: 300}, {x: 240, y: 64, width: 700, height: 640})).toEqual({x: 240, y: 64, width: 700, height: 216});
  expect(softwareVisibleViewport({x: 240, y: -400, width: 700, height: 300}, {x: 240, y: 64, width: 700, height: 640})).toBeNull();
 });
});
