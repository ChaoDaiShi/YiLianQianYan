import { describe, expect, it } from "vitest";
import type { ExecutionAuditEvent } from "../../api/executionAudit";
import type { ExecutionRecord } from "./model";
import { groupTimelinePoints, layoutTimelineLabels, projectTimelineTracks, projectTurnTimeline, timelineViewport } from "./turnTimeline";

const turn = { id: "turn", user_message_id: "user", status: "completed", started_at: 1000, finished_at: 6000 };
const action: ExecutionRecord = {
  toolCallId: "write", conversationId: "chat", name: "write_file", args: { path: "本周周报.md" },
  riskLevel: "medium", approvalStatus: "approved", executionStatus: "succeeded", verificationStatus: "passed", sequence: 1,
};
const event = (event_type: string, created_at: number, decision_status: string, tool_call_id = "write"): ExecutionAuditEvent => ({
  event_type, created_at, decision_status, tool_call_id,
});

describe("chat turn timeline", () => {
  it("zooms around the selected time and clamps the visible window to the turn", () => {
    expect(timelineViewport(6000, 4, 5400)).toEqual({ start: 4500, end: 6000 });
    expect(timelineViewport(6000, 4, 3200)).toEqual({ start: 2450, end: 3950 });
    expect(timelineViewport(6000, 1, 5400)).toEqual({ start: 0, end: 6000 });
  });

  it("separates close labels without moving their actual time positions", () => {
    expect(layoutTimelineLabels([0, 6, 64, 65, 65, 91], 16)).toEqual([0, 1, 0, 1, 2, 0]);
  });

  it("groups only within the available visual width and leaves expanded points individually selectable", () => {
    const positions = [0, 3, 7, 90, 91, 91].map(x => ({ x }));
    expect(groupTimelinePoints(positions, 5).map(group => group.map(point => point.x))).toEqual([[0, 3], [7], [90, 91, 91]]);
    expect(groupTimelinePoints(positions.map(point => ({ ...point, expanded: point.x >= 90 })), 5).map(group => group.length)).toEqual([2, 1, 1, 1, 1]);
  });
  it("uses recorded times and preserves causal order within the same millisecond", () => {
    const nodes = projectTurnTimeline(turn, [action], [
      event("verification_finished", 3200, "verified"), event("execution_finished", 3200, "succeeded"),
      event("execution_started", 3100, "started"), event("approval_resolved", 3100, "approved"),
      event("approval_requested", 1200, "require_approval"), event("approval_requested", 900, "require_approval"),
      event("execution_started", 1050, "started", "another-call"),
    ], []);
    expect(nodes.map(({ label, at }) => [label, at])).toEqual([
      ["等待审批", 1200], ["已批准", 3100], ["开始执行", 3100], ["执行结束", 3200], ["已核验", 3200],
    ]);
  });

  it("distinguishes rejection from execution and leaves an unrecorded decision time empty", () => {
    const rejected = { ...action, approvalStatus: "rejected", executionStatus: "rejected", verificationStatus: "not_requested" } as const;
    expect(projectTurnTimeline(turn, [rejected], [event("approval_requested", 1200, "require_approval")], [])).toMatchObject([
      { label: "等待审批", at: 1200 }, { label: "已拒绝", at: undefined },
    ]);
  });

  it("keeps a policy denial distinct from a failed verification after execution", () => {
    const failed = { ...action, executionStatus: "failed", verificationStatus: "not_requested" } as const;
    expect(projectTurnTimeline(turn, [{ ...failed, startedAt: 2000 }], [event("policy_decided", 2100, "deny")], []).map(node => node.label)).toEqual([
      "等待审批", "已批准", "执行被拦截",
    ]);
    const unverified = { ...failed, verificationStatus: "failed" } as const;
    expect(projectTurnTimeline(turn, [unverified], [event("execution_finished", 3200, "succeeded"), event("verification_finished", 3201, "verification_failed")], []).map(node => node.label)).toEqual([
      "等待审批", "已批准", "执行结束", "核验失败",
    ]);
  });

  it("measures approval wait and tool execution separately and keeps the recorded failure reason", () => {
    const failed = { ...action, executionStatus: "failed", verificationStatus: "failed", verificationReason: "文件内容与预期不符" } as const;
    const nodes = projectTurnTimeline(turn, [failed], [
      event("approval_requested", 1200, "require_approval"), event("approval_resolved", 3100, "approved"),
      event("execution_started", 3400, "started"), event("execution_finished", 3500, "succeeded"),
      event("verification_finished", 3550, "verification_failed"),
    ], []);
    expect(projectTimelineTracks(nodes, turn, 6000)[0]).toMatchObject({
      approval: { start: 1200, end: 3100 },
      execution: { start: 3400, end: 3500 },
      verification: { reason: "文件内容与预期不符" },
    });
  });

  it("counts a live wait, but never treats the policy recheck as time spent executing", () => {
    const open = { ...turn, finished_at: null };
    const pending = { ...action, approvalStatus: "pending", executionStatus: "awaiting_approval", verificationStatus: "not_requested" } as const;
    const pendingNodes = projectTurnTimeline(open, [pending], [event("approval_requested", 1200, "require_approval")], []);
    expect(projectTimelineTracks(pendingNodes, open, 2700)[0].approval).toMatchObject({ start: 1200, end: 2700, live: true });

    const blocked = { ...action, startedAt: 3100, finishedAt: 3400, executionStatus: "failed", verificationStatus: "not_requested" } as const;
    const denial = { ...event("policy_decided", 3400, "deny"), details: { reason: "工作空间写入权限已撤销" } };
    const blockedNodes = projectTurnTimeline(turn, [blocked], [denial], []);
    expect(projectTimelineTracks(blockedNodes, turn, 6000)[0].execution).toMatchObject({
      start: undefined, end: undefined, node: { label: "执行被拦截", reason: "工作空间写入权限已撤销" },
    });
  });
});
