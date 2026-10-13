import { useCallback, useState } from "react";
import {
  AlertTriangle,
  GitBranch,
  ShieldCheck,
  Wrench,
  X,
} from "lucide-react";
import {
  applyRepair,
  type ApplyRepairResponse,
  type RepairProposal,
} from "../../../api/proofloop";
import { Badge, Button } from "../../../components/ui";
import Identifier from "../../../components/Identifier";
import { formatIdentifierText, type IdentifierReference } from "../../../utils/identifiers";
import { formatRepairOperation } from "./proofloopModel";

export interface RepairReviewPanelProps {
  proposal: RepairProposal;
  currentGraphRevision: number;
  onApplyRepair: (response: ApplyRepairResponse) => void;
  onCancel?: () => void;
  className?: string;
}

export default function RepairReviewPanel({
  proposal,
  currentGraphRevision,
  onApplyRepair,
  onCancel,
  className = "",
}: RepairReviewPanelProps) {
  const [applying, setApplying] = useState(false);
  const [applyError, setApplyError] = useState<string | null>(null);

  const isStaleRevision = currentGraphRevision !== proposal.expected_graph_revision;
  const references: IdentifierReference[] = [
    { kind: "repair", value: proposal.repair_id },
    { kind: "report", value: proposal.source_report_id },
    ...proposal.failure_execution_ids.map(value => ({ kind: "execution" as const, value })),
    ...proposal.affected_node_ids.map(value => ({ kind: "node" as const, value })),
  ];

  const handleApply = useCallback(async () => {
    if (isStaleRevision || applying) return;
    setApplying(true);
    setApplyError(null);

    try {
      const result = await applyRepair(proposal.repair_id, {
        expected_graph_revision: proposal.expected_graph_revision,
      });

      if (result.ok) {
        onApplyRepair(result.data);
      } else {
        setApplyError(result.error.message || "应用修复提案失败。");
      }
    } catch (err: unknown) {
      setApplyError(err instanceof Error ? err.message : String(err));
    } finally {
      setApplying(false);
    }
  }, [proposal.repair_id, proposal.expected_graph_revision, isStaleRevision, applying, onApplyRepair]);

  return (
    <div className={`repair-review-panel flex flex-col gap-4 rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface)] p-4 ${className}`}>
      {/* Header */}
      <div className="flex items-center justify-between border-b border-[var(--border-soft)] pb-3">
        <div className="flex items-center gap-2">
          <Wrench className="h-5 w-5 text-[var(--accent)]" />
          <h2 className="text-base font-semibold text-[var(--text)]">受控局部修复审查</h2>
          <Badge tone="info">第 {proposal.round} 轮修复</Badge>
        </div>
        {onCancel && (
          <button
            type="button"
            onClick={onCancel}
            aria-label="关闭修复审查"
            className="text-[var(--text-faint)] hover:text-[var(--text)]"
          >
            <X size={18} />
          </button>
        )}
      </div>

      {/* Rationale and failure context */}
      <div className="rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-elevated)] p-3 text-xs">
        <span className="font-medium text-[var(--text)]">修复理由与修改依据：</span>
        <p className="mt-1 break-words text-[var(--text-secondary)]">{formatIdentifierText(proposal.rationale, references)}</p>
        <div className="mt-2 flex flex-wrap gap-x-3 gap-y-2 text-[11px] text-[var(--text-faint)]">
          <span>提案 <Identifier kind="repair" value={proposal.repair_id} /></span>
          <span>源报告 <Identifier kind="report" value={proposal.source_report_id} /></span>
          <span>关联失败执行：{proposal.failure_execution_ids.length ? proposal.failure_execution_ids.map(id => <Identifier key={id} kind="execution" value={id} className="ml-1" />) : "无"}</span>
        </div>
      </div>

      {/* Revision conflict warning */}
      {isStaleRevision && (
        <div className="flex items-start gap-2 rounded-[var(--radius-md)] border border-[var(--danger-border)] bg-[var(--danger-soft)] p-3 text-xs text-[var(--danger-fg)]" role="alert">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div>
            <p className="font-medium">任务图版本已变更</p>
            <p className="mt-0.5">
              提案基于版本 r{proposal.expected_graph_revision} 生成，但当前图版本已更新为 r{currentGraphRevision}。该修复提案已失效，请重新生成修复建议。
            </p>
          </div>
        </div>
      )}

      {/* Affected subtree */}
      <div className="flex flex-col gap-1.5 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-elevated)] p-3 text-xs">
        <div className="flex items-center gap-1.5 font-medium text-[var(--text)]">
          <GitBranch className="h-4 w-4 text-[var(--accent)]" />
          <span>受控重跑边界</span>
        </div>
        <div className="text-[var(--text-secondary)]">
          <span>失败根节点：</span>
          <Identifier kind="node" value={proposal.rerun_root_node_id} />
        </div>
        <div className="text-[var(--text-secondary)]">
          <span>重跑影响节点（共 {proposal.affected_node_ids.length} 个）：</span>
          <span className="inline-flex flex-wrap gap-2 text-[var(--text)]">{proposal.affected_node_ids.map(id => <Identifier key={id} kind="node" value={id} />)}</span>
        </div>
        <p className="mt-1 text-[11px] text-[var(--text-faint)]">
          保护机制：仅重跑受影响下游分支；无关成功分支与节点保持不变，旧 execution attempt 完整保留。
        </p>
      </div>

      {/* Operations list */}
      <div className="flex flex-col gap-2">
        <h3 className="text-xs font-semibold text-[var(--text)]">
          变更操作列表（共 {proposal.operations.length} 项）
        </h3>
        <div className="flex flex-col gap-1.5">
          {proposal.operations.map((op, idx) => {
            const formatted = formatRepairOperation(op);
            return (
              <div
                key={idx}
                className="flex items-start justify-between rounded border border-[var(--border-soft)] bg-[var(--surface)] p-2 text-xs"
              >
                <div>
                  <div className="flex items-center gap-2">
                    <Badge tone="default">{formatted.kindLabel}</Badge>
                    <Identifier kind="node" value={formatted.nodeId} />
                  </div>
                  <p className="mt-1 break-words text-[var(--text-secondary)]">{formatIdentifierText(formatted.summary, references)}</p>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {applyError && (
        <div className="rounded-[var(--radius-md)] border border-[var(--danger-border)] bg-[var(--danger-soft)] p-2 text-xs text-[var(--danger-fg)]" role="alert">
          {applyError}
        </div>
      )}

      {/* Footer action */}
      <div className="flex flex-col gap-2 border-t border-[var(--border-soft)] pt-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-1 text-[11px] text-[var(--text-faint)]">
          <ShieldCheck className="h-3.5 w-3.5 text-[var(--success)]" />
          <span>应用修复不扩大现有权限，重跑受原有审批保护</span>
        </div>
        <div className="flex gap-2">
          {onCancel && (
            <Button size="sm" variant="ghost" onClick={onCancel}>
              取消
            </Button>
          )}
          <Button
            size="sm"
            disabled={isStaleRevision || applying}
            onClick={() => void handleApply()}
          >
            {applying ? "应用中…" : "确认并应用局部修复"}
          </Button>
        </div>
      </div>
    </div>
  );
}
