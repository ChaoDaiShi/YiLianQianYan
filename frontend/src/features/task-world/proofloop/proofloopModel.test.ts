import { describe, expect, it } from "vitest";
import type {
  Criterion,
  ProofDraft,
  ProofError,
  RepairOperation,
} from "../../../api/proofloop";
import {
  formatCriterionCheckDescription,
  formatCriterionStatus,
  formatDiagnosticMessage,
  formatDraftStatus,
  formatGoalOverallStatus,
  formatPendingInputReason,
  formatRepairOperation,
  isDraftSubmittable,
} from "./proofloopModel";

describe("proofloopModel", () => {
  it("formats draft statuses with appropriate tones and descriptions", () => {
    expect(formatDraftStatus("ready_for_confirmation")).toEqual({
      tone: "success",
      label: "可确认保存",
      description: expect.stringContaining("已通过全部静态校验"),
    });

    expect(formatDraftStatus("needs_input")).toEqual({
      tone: "warning",
      label: "待补充参数",
      description: expect.stringContaining("未填写的必须输入"),
    });

    expect(formatDraftStatus("invalid")).toEqual({
      tone: "danger",
      label: "计划不合法",
      description: expect.stringContaining("存在类型、依赖或能力定义错误"),
    });
  });

  it("translates diagnostic error codes to friendly titles and details", () => {
    const error: ProofError = {
      code: "proofloop_capability_unavailable",
      message: "未找到工具 mcp__fetch",
      node_id: "node_1",
      target_path: ["url"],
    };
    const formatted = formatDiagnosticMessage(error);
    expect(formatted.title).toBe("所需能力不可用");
    expect(formatted.detail).toContain("node_1");
    expect(formatted.detail).toContain("url");
    expect(formatted.detail).toContain("未找到工具 mcp__fetch");
  });

  it("formats pending input reasons correctly", () => {
    expect(formatPendingInputReason("missing_literal")).toBe("缺少固定必填参数");
    expect(formatPendingInputReason("unknown_output")).toBe(
      "上游未声明输出结构，需人工指定数据路径",
    );
  });

  it("formats criterion statuses and overall goal outcomes", () => {
    expect(formatCriterionStatus("pass")).toEqual({ tone: "success", label: "通过" });
    expect(formatCriterionStatus("fail")).toEqual({ tone: "danger", label: "未达成" });
    expect(formatCriterionStatus("human_pending")).toEqual({
      tone: "warning",
      label: "待人工审核",
    });

    expect(formatGoalOverallStatus("pass").tone).toBe("success");
    expect(formatGoalOverallStatus("fail").tone).toBe("danger");
    expect(formatGoalOverallStatus("incomplete").tone).toBe("warning");
  });

  it("formats criterion check descriptions accurately", () => {
    const c1: Criterion = {
      id: "c1",
      description: "文档必须存在",
      required: true,
      check: { kind: "artifact_exists", deliverable_id: "doc1" },
    };
    expect(formatCriterionCheckDescription(c1)).toContain("doc1");

    const c2: Criterion = {
      id: "c2",
      description: "小节齐全",
      required: true,
      check: { kind: "required_sections", deliverable_id: "doc1", headings: ["简介", "方案"] },
    };
    expect(formatCriterionCheckDescription(c2)).toContain("简介、方案");
  });

  it("formats repair operations clearly", () => {
    const op: RepairOperation = {
      kind: "replace_literal",
      node_id: "node_a",
      target_path: ["timeout"],
      value: 5000,
    };
    const formatted = formatRepairOperation(op);
    expect(formatted.kindLabel).toBe("修改固定参数");
    expect(formatted.nodeId).toBe("node_a");
    expect(formatted.summary).toContain("5000");
  });

  it("checks if a draft is submittable", () => {
    expect(isDraftSubmittable(null)).toBe(false);

    const draft = {
      status: "ready_for_confirmation",
      compilation: { status: "ready" },
    } as unknown as ProofDraft;
    expect(isDraftSubmittable(draft)).toBe(true);

    const incompleteDraft = {
      status: "needs_input",
      compilation: { status: "needs_input" },
    } as unknown as ProofDraft;
    expect(isDraftSubmittable(incompleteDraft)).toBe(false);
  });
});
