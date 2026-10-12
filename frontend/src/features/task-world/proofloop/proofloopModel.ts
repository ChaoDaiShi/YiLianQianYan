import type {
  AcceptanceReport,
  Criterion,
  CriterionResult,
  FieldPath,
  PendingInput,
  ProofDraft,
  ProofError,
  RepairOperation,
} from "../../../api/proofloop";

export type DraftDisplayStatus = {
  tone: "default" | "info" | "warning" | "danger" | "success";
  label: string;
  description: string;
};

export function formatDraftStatus(status: ProofDraft["status"]): DraftDisplayStatus {
  switch (status) {
    case "proposed":
      return {
        tone: "info",
        label: "规划就绪",
        description: "模型已生成初始候选计划，等待静态编译和参数检查。",
      };
    case "needs_input":
      return {
        tone: "warning",
        label: "待补充参数",
        description: "存在未填写的必须输入或无法推导的输出字段，需要人工确认。",
      };
    case "invalid":
      return {
        tone: "danger",
        label: "计划不合法",
        description: "计划存在类型、依赖或能力定义错误，无法保存或执行。",
      };
    case "ready_for_confirmation":
      return {
        tone: "success",
        label: "可确认保存",
        description: "目标与计划已通过全部静态校验，可保存至任务画布。",
      };
    case "confirmed":
      return {
        tone: "success",
        label: "已确认保存",
        description: "计划已保存到任务画布，可按照原有流程启动与审批执行。",
      };
    default:
      return {
        tone: "default",
        label: "未知状态",
        description: "未识别的草稿状态。",
      };
  }
}

export function formatFieldPath(path: FieldPath): string {
  if (!path || path.length === 0) return "(根路径)";
  return path.join(".");
}

export function formatDiagnosticMessage(diagnostic: ProofError): {
  title: string;
  detail: string;
  code: string;
} {
  const target = diagnostic.target_path ? `（参数: ${formatFieldPath(diagnostic.target_path)}）` : "";
  const node = diagnostic.node_id ? `节点 ${diagnostic.node_id}${target}: ` : "";

  switch (diagnostic.code) {
    case "proofloop_capability_unavailable":
      return {
        code: diagnostic.code,
        title: "所需能力不可用",
        detail: `${node}${diagnostic.message || "计划引用的能力在当前环境不可用或已被禁用。"}`,
      };
    case "proofloop_unknown_output":
      return {
        code: diagnostic.code,
        title: "工具输出结构未知",
        detail: `${node}${diagnostic.message || "上游工具未声明 output_schema，无法建立自动类型绑定。"}`,
      };
    case "proofloop_unsupported_schema":
      return {
        code: diagnostic.code,
        title: "数据结构不受支持",
        detail: `${node}${diagnostic.message || "字段类型或嵌套深度超出了当前校验器支持的范围。"}`,
      };
    case "proofloop_goal_uncovered":
      return {
        code: diagnostic.code,
        title: "目标交付物未完全覆盖",
        detail: `${node}${diagnostic.message || "候选计划未提供产出所有目标交付物的节点或步骤。"}`,
      };
    case "proofloop_stale_revision":
      return {
        code: diagnostic.code,
        title: "草稿版本冲突",
        detail: `${node}${diagnostic.message || "图或草稿已被更新，请刷新后重试。"}`,
      };
    case "proofloop_evidence_stale":
      return {
        code: diagnostic.code,
        title: "证据快照过期",
        detail: `${node}${diagnostic.message || "所引用的执行或产物已变更，需要重新进行目标验收。"}`,
      };
    case "proofloop_budget_exhausted":
      return {
        code: diagnostic.code,
        title: "修复预算耗尽",
        detail: `${node}${diagnostic.message || "模型调用次数或修复轮次已达上限，需要人工接管。"}`,
      };
    default:
      return {
        code: diagnostic.code,
        title: "编译提示",
        detail: `${node}${diagnostic.message}`,
      };
  }
}

export function formatPendingInputReason(reason: PendingInput["reason"]): string {
  switch (reason) {
    case "missing_literal":
      return "缺少固定必填参数";
    case "unknown_output":
      return "上游未声明输出结构，需人工指定数据路径";
    case "unsupported_schema":
      return "暂不支持自动绑定的复合类型";
    default:
      return "参数待确定";
  }
}

export type CriterionDisplayStatus = {
  tone: "default" | "info" | "warning" | "danger" | "success";
  label: string;
};

export function formatCriterionStatus(status: CriterionResult["status"]): CriterionDisplayStatus {
  switch (status) {
    case "pass":
      return { tone: "success", label: "通过" };
    case "fail":
      return { tone: "danger", label: "未达成" };
    case "human_pending":
      return { tone: "warning", label: "待人工审核" };
    case "not_checked":
      return { tone: "default", label: "未检查" };
    case "stale":
      return { tone: "warning", label: "证据过期" };
    default:
      return { tone: "default", label: "未知" };
  }
}

export function formatGoalOverallStatus(status: AcceptanceReport["status"]): {
  tone: "danger" | "warning" | "success";
  label: string;
  description: string;
} {
  switch (status) {
    case "pass":
      return {
        tone: "success",
        label: "目标已达成",
        description: "所有必需条件均已通过并具有确凿的成果证据。",
      };
    case "fail":
      return {
        tone: "danger",
        label: "目标未达成",
        description: "部分必需验收条件未满足，可查看具体失败项或请求受控修复。",
      };
    case "incomplete":
      return {
        tone: "warning",
        label: "验收未完成",
        description: "存在未检查或等待人工确认的准则项，尚未形成最终结论。",
      };
    default:
      return {
        tone: "warning",
        label: "未定结论",
        description: "验收报告状态不明确。",
      };
  }
}

export function formatCriterionCheckDescription(criterion: Criterion): string {
  const { check } = criterion;
  switch (check.kind) {
    case "artifact_exists":
      return `要求生成交付物文件（ID: ${check.deliverable_id}）`;
    case "artifact_digest_matches":
      return `要求交付物内容摘要 SHA-256 匹配（ID: ${check.deliverable_id}）`;
    case "required_sections":
      return `要求包含小节: ${check.headings.join("、")}`;
    case "source_traceability":
      return `要求至少追溯 ${check.min_sources} 个可查证信息来源`;
    case "human_review":
      return `人工审核标准: ${check.rubric}`;
    default:
      return "自定义检查项";
  }
}

export interface FormattedRepairOperation {
  kindLabel: string;
  summary: string;
  nodeId: string;
}

export function formatRepairOperation(op: RepairOperation): FormattedRepairOperation {
  switch (op.kind) {
    case "replace_literal":
      return {
        kindLabel: "修改固定参数",
        nodeId: op.node_id,
        summary: `更新参数 [${formatFieldPath(op.target_path)}] = ${JSON.stringify(op.value)}`,
      };
    case "replace_binding":
      return {
        kindLabel: "修改输入绑定",
        nodeId: op.node_id,
        summary: `参数 [${formatFieldPath(op.binding.target_path)}] 改由节点 ${op.binding.source.node_id} 的输出 [${formatFieldPath(op.binding.source.path)}] 提供`,
      };
    case "replace_capability":
      return {
        kindLabel: "替换执行能力",
        nodeId: op.node_id,
        summary: `将能力替换为 [${op.capability_id}]，同时调整参数与依赖绑定`,
      };
    case "rerun_node":
      return {
        kindLabel: "单节点重跑",
        nodeId: op.node_id,
        summary: `保持原参数配置，重新调度执行该节点`,
      };
  }
}

export function isDraftSubmittable(draft: ProofDraft | null): boolean {
  if (!draft) return false;
  return draft.status === "ready_for_confirmation" && draft.compilation.status === "ready";
}

export function canProposeRepair(report: AcceptanceReport | null): boolean {
  if (!report) return false;
  return report.status === "fail" || report.status === "incomplete";
}
