import { useCallback, useState } from "react";
import {
  AlertTriangle,
  FileText,
  Info,
  ListTodo,
  Save,
} from "lucide-react";
import {
  confirmProofDraft,
  getProofDraft,
  updateProofDraft,
  type ConfirmProofDraftResponse,
  type PendingInput,
  type ProofDraft,
} from "../../../api/proofloop";
import { Badge, Button, Input } from "../../../components/ui";
import Identifier from "../../../components/Identifier";
import { formatIdentifier } from "../../../utils/identifiers";
import PlanComparisonPanel from "./PlanComparisonPanel";
import {
  formatDiagnosticMessage,
  formatDraftStatus,
  formatFieldPath,
  formatPendingInputReason,
  isDraftSubmittable,
  withFixedInput,
} from "./proofloopModel";

export interface PlanReviewPanelProps {
  draft: ProofDraft;
  onUpdateDraft?: (updated: ProofDraft) => void;
  onConfirmed: (result: ConfirmProofDraftResponse) => void;
  onBackToEdit?: () => void;
  className?: string;
}

export default function PlanReviewPanel({
  draft,
  onUpdateDraft,
  onConfirmed,
  onBackToEdit,
  className = "",
}: PlanReviewPanelProps) {
  const [confirming, setConfirming] = useState(false);
  const [confirmError, setConfirmError] = useState<string | null>(null);

  const [inputValues, setInputValues] = useState<Record<string, string>>({});
  const [updating, setUpdating] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [pendingCandidateSelection, setPendingCandidateSelection] = useState<string | null>(null);

  const statusMeta = formatDraftStatus(draft.status);
  const selectionRequired = Boolean(draft.planning_review && !draft.planning_review.selected_candidate_id);
  const selectionUnresolved = Boolean(draft.planning_review && pendingCandidateSelection);
  const canConfirm = isDraftSubmittable(draft) && !selectionRequired && !selectionUnresolved && !confirming && !updating;

  const handleConfirm = useCallback(async () => {
    if (!canConfirm) return;
    setConfirming(true);
    setConfirmError(null);

    try {
      const result = await confirmProofDraft(draft.draft_id, {
        expected_revision: draft.revision,
      });

      if (result.ok) {
        onConfirmed(result.data);
      } else {
        setConfirmError(result.error.message || "确认保存任务图失败，请稍后重试。");
      }
    } catch (err: unknown) {
      setConfirmError(err instanceof Error ? err.message : String(err));
    } finally {
      setConfirming(false);
    }
  }, [draft.draft_id, draft.revision, canConfirm, onConfirmed]);

  const handleUpdatePendingInput = async (pending: PendingInput) => {
    if (!draft.candidate) return;
    const inputKey = `${pending.node_id}:${formatFieldPath(pending.target_path)}`;
    const value = inputValues[inputKey];
    if (value === undefined || value.trim() === "") return;

    setUpdating(true);
    setUpdateError(null);

    try {
      let parsedValue: unknown = value;
      try {
        parsedValue = JSON.parse(value);
      } catch {
        // Keep as string if not valid JSON
        parsedValue = value;
      }

      const updatedCandidate = withFixedInput(draft.candidate, pending, parsedValue);

      const result = await updateProofDraft(draft.draft_id, {
        expected_revision: draft.revision,
        contract: draft.contract,
        candidate: updatedCandidate,
        binding_confirmations: draft.binding_confirmations.filter((confirmation) => !(confirmation.node_id === pending.node_id && JSON.stringify(confirmation.target_path) === JSON.stringify(pending.target_path))),
      });

      if (result.ok) {
        onUpdateDraft?.(result.data);
      } else {
        setUpdateError(result.error.message || "更新草稿参数失败。");
      }
    } catch (err: unknown) {
      setUpdateError(err instanceof Error ? err.message : String(err));
    } finally {
      setUpdating(false);
    }
  };

  const handleBudgetChange = async (timeBasis: "active_work" | "wall_clock", milliseconds: number) => {
    if (!Number.isFinite(milliseconds) || milliseconds <= 0) return;
    setUpdating(true);
    setUpdateError(null);
    try {
      const result = await updateProofDraft(draft.draft_id, {
        expected_revision: draft.revision,
        contract: { ...draft.contract, budget: { ...draft.contract.budget, time_basis: timeBasis, max_wall_time_ms: milliseconds } },
        candidate: draft.candidate,
        binding_confirmations: draft.binding_confirmations,
      });
      if (result.ok) onUpdateDraft?.(result.data);
      else setUpdateError(result.error.message || "保存时间预算失败，请加载最新草稿后重试。");
    } catch (error: unknown) {
      setUpdateError(error instanceof Error ? error.message : "保存时间预算失败。");
    } finally { setUpdating(false); }
  };

  const loadLatestDraft = async () => {
    setUpdating(true);
    try {
      const result = await getProofDraft(draft.draft_id);
      if (result.ok) { onUpdateDraft?.(result.data); setConfirmError(null); setUpdateError(null); }
      else setUpdateError(result.error.message);
    } catch (error: unknown) {
      setUpdateError(error instanceof Error ? error.message : "读取最新草稿失败。");
    } finally { setUpdating(false); }
  };

  return (
    <div className={`plan-review-panel flex flex-col gap-4 rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface)] p-4 ${className}`}>
      {/* Header and status badge */}
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--border-soft)] pb-3">
        <div>
          <div className="flex items-center gap-2">
            <h2 className="text-base font-semibold text-[var(--text)]">任务候选计划评审</h2>
            <Badge tone={statusMeta.tone}>{statusMeta.label}</Badge>
          </div>
          <p className="mt-1 text-xs text-[var(--text-faint)]">{statusMeta.description}</p>
        </div>
        {onBackToEdit && (
          <Button size="sm" variant="secondary" onClick={onBackToEdit}>
            调整目标或能力
          </Button>
        )}
      </div>

      {draft.planning_review && <PlanComparisonPanel draft={draft} onDraftChange={onUpdateDraft} onSelectionIntentChange={setPendingCandidateSelection} />}
      {selectionRequired && (
        <p className="rounded border border-[var(--warning-border)] bg-[var(--warning-soft)] p-3 text-xs text-[var(--warning-fg)]" role="status">
          请先在方案比较中明确选择一个可用方案，再确认保存到画布。推荐方案不会自动选中。
        </p>
      )}
      {selectionUnresolved && !selectionRequired && (
        <p className="rounded border border-[var(--warning-border)] bg-[var(--warning-soft)] p-3 text-xs text-[var(--warning-fg)]" role="status">
          正在保存你选择的方案。保存完成或重新选择后，才能确认当前草稿。
        </p>
      )}

      {/* Goal Contract summary */}
      <div className="rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-elevated)] p-3 text-xs">
        <div className="flex items-center gap-2 font-medium text-[var(--text)]">
          <FileText className="h-4 w-4 text-[var(--accent)]" />
          <span>目标契约约定（版本 v{draft.contract.version}）</span>
        </div>
        <p className="mt-1 text-[var(--text-secondary)]">“{draft.contract.user_goal}”</p>

        <div className="mt-3 grid grid-cols-1 gap-2 sm:grid-cols-2">
          <div className="rounded border border-[var(--border-soft)] p-2">
            <span className="font-medium text-[var(--text)]">交付物要求（{draft.contract.deliverables.length} 项）</span>
            <ul className="mt-1 list-disc pl-4 text-[var(--text-secondary)]">
              {draft.contract.deliverables.map((d) => (
                <li key={d.id}>
                  {d.title} <span className="text-[var(--text-faint)]">({d.mime_type})</span>
                </li>
              ))}
            </ul>
          </div>

          <div className="rounded border border-[var(--border-soft)] p-2">
            <span className="font-medium text-[var(--text)]">验收准则（{draft.contract.criteria.length} 项）</span>
            <ul className="mt-1 list-disc pl-4 text-[var(--text-secondary)]">
              {draft.contract.criteria.map((c) => (
                <li key={c.id}>
                  {c.description} {c.required ? <span className="text-[var(--danger)]">*必需</span> : "(可选)"}
                </li>
              ))}
            </ul>
          </div>
        </div>

        <div className="mt-2 flex flex-wrap gap-4 text-[11px] text-[var(--text-faint)]">
          <span>预算：模型调用 ≤ {draft.contract.budget.max_model_calls} 次</span>
          <span>总 Token ≤ {draft.contract.budget.max_total_tokens}</span>
          <span>修复轮次 ≤ {draft.contract.budget.max_repair_rounds} 轮</span>
          <span>{draft.contract.budget.time_basis === "wall_clock" ? "总时限" : "活动预算"}：{draft.contract.budget.max_wall_time_ms / 60_000} 分钟</span>
        </div>
        <div className="mt-3 flex flex-wrap items-center gap-3 text-xs">
          <label>时间口径 <select aria-label="时间预算口径" disabled={updating || confirming || draft.status === "confirmed"} value={draft.contract.budget.time_basis || "active_work"}
            onChange={(event) => void handleBudgetChange(event.target.value as "active_work" | "wall_clock", draft.contract.budget.max_wall_time_ms)}>
            <option value="active_work">活动时间，审阅与审批等待不计入</option>
            <option value="wall_clock">总经过时间，包含人工等待</option>
          </select></label>
          <label>上限（分钟） <Input aria-label="时间预算上限分钟" type="number" min="1" step="1" className="w-24" defaultValue={draft.contract.budget.max_wall_time_ms / 60_000}
            key={`${draft.draft_id}:${draft.revision}:budget`} disabled={updating || confirming || draft.status === "confirmed"}
            onBlur={(event) => { const milliseconds = Number(event.target.value) * 60_000; if (milliseconds !== draft.contract.budget.max_wall_time_ms) void handleBudgetChange(draft.contract.budget.time_basis || "active_work", milliseconds); }} /></label>
        </div>
        <p className="mt-2 text-[11px] text-[var(--text-faint)]">{draft.contract.budget.time_basis === "wall_clock" ? "从规划开始计时，审阅、审批和暂停均占用总时限。" : "只累计规划和实际执行的活动时间，审阅计划、补充参数与等待审批不会消耗活动预算。"}</p>
      </div>

      {/* Diagnostics if any */}
      {draft.compilation.diagnostics.length > 0 && (
        <div className="flex flex-col gap-2 rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-soft)] p-3 text-xs">
          <div className="flex items-center gap-1.5 font-medium text-[var(--warning-fg)]">
            <AlertTriangle className="h-4 w-4" />
            <span>编译检查报告（{draft.compilation.diagnostics.length} 项）</span>
          </div>
          <div className="flex flex-col gap-1">
            {draft.compilation.diagnostics.map((diag, index) => {
              const formatted = formatDiagnosticMessage(diag);
              return (
                <div key={index} className="rounded border border-[var(--border-soft)] bg-[var(--surface)] p-2">
                  <div className="font-medium text-[var(--text)]">{formatted.title}</div>
                  <div className="text-[var(--text-secondary)]">{formatted.detail}</div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Pending Inputs to complete */}
      {draft.compilation.pending_inputs.length > 0 && (
        <div className="flex flex-col gap-2 rounded-[var(--radius-md)] border border-[var(--accent-border)] bg-[var(--surface-elevated)] p-3 text-xs">
          <div className="flex items-center gap-1.5 font-medium text-[var(--accent)]">
            <Info className="h-4 w-4" />
            <span>需要人工补充或确认的参数（{draft.compilation.pending_inputs.length} 项）</span>
          </div>
          <div className="flex flex-col gap-2">
            {draft.compilation.pending_inputs.map((pending, idx) => {
              const inputKey = `${pending.node_id}:${formatFieldPath(pending.target_path)}`;
              return (
                <div key={idx} className="flex flex-col gap-1 rounded border border-[var(--border-soft)] bg-[var(--surface)] p-2">
                  <div className="flex items-center justify-between">
                    <span className="font-medium text-[var(--text)]">
                      节点 <Identifier kind="node" value={pending.node_id} /> → 参数 [{formatFieldPath(pending.target_path)}]
                    </span>
                    <span className="text-[11px] text-[var(--warning-fg)]">
                      {formatPendingInputReason(pending.reason)}
                    </span>
                  </div>
                  <div className="mt-1 flex gap-2">
                    <Input
                      aria-label={`补充参数 ${formatFieldPath(pending.target_path)}`}
                      value={inputValues[inputKey] || ""}
                      onChange={(e) =>
                        setInputValues((prev) => ({ ...prev, [inputKey]: e.target.value }))
                      }
                      placeholder="输入固定值（支持字符串或 JSON）"
                      className="text-xs"
                      disabled={updating}
                    />
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={updating || !inputValues[inputKey]?.trim()}
                      onClick={() => void handleUpdatePendingInput(pending)}
                    >
                      {updating ? "更新中…" : "补充并重验"}
                    </Button>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {updateError && (
        <div className="rounded-[var(--radius-md)] border border-[var(--danger-border)] bg-[var(--danger-soft)] p-2 text-xs text-[var(--danger-fg)]" role="alert">
          {updateError}
        </div>
      )}

      {/* Candidate Plan Nodes and Flow */}
      {draft.candidate && (
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-1.5 text-xs font-medium text-[var(--text)]">
            <ListTodo className="h-4 w-4 text-[var(--accent)]" />
            <span>候选编排计划（{draft.candidate.nodes.length} 个任务节点）</span>
          </div>
          <div className="grid gap-2">
            {draft.candidate.nodes.map((node, i) => (
              <div key={node.id} className="flex flex-col gap-1 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface)] p-3 text-xs">
                <div className="flex items-center justify-between">
                  <span className="font-semibold text-[var(--text)]">
                    {i + 1}. {node.title} <Identifier kind="node" value={node.id} className="font-normal text-[var(--text-faint)]" />
                  </span>
                  <Badge tone="default">能力: {node.capability_id}</Badge>
                </div>
                {node.instruction && (
                  <p className="text-[var(--text-secondary)]">{node.instruction}</p>
                )}
                {node.bindings.length > 0 && (
                  <div className="mt-1 rounded bg-[var(--surface-elevated)] p-1.5 text-[11px] text-[var(--text-faint)]">
                    <span className="font-medium text-[var(--text-secondary)]">输入绑定：</span>
                    {node.bindings.map((b, bi) => (
                      <span key={bi} className="ml-2 inline-flex items-center gap-1">
                        [{formatFieldPath(b.target_path)}] ← <span title={b.source.node_id}>{formatIdentifier("node", b.source.node_id)}</span>.[{formatFieldPath(b.source.path)}]
                      </span>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {confirmError && (
        <div className="rounded-[var(--radius-md)] border border-[var(--danger-border)] bg-[var(--danger-soft)] p-3 text-xs text-[var(--danger-fg)]" role="alert">
          {confirmError}
        </div>
      )}

      {/* Confirmation footer */}
      {(confirmError || updateError) && <Button size="sm" variant="secondary" disabled={updating} onClick={() => void loadLatestDraft()}>加载最新草稿（保留输入）</Button>}
      {draft.status === "confirmed" && draft.confirmed_graph_revision !== null && <Button size="sm" onClick={() => onConfirmed({ graph_id: draft.graph_id, graph_revision: draft.confirmed_graph_revision!, goal_id: draft.contract.goal_id, goal_version: draft.contract.version })}>打开已确认任务图</Button>}
      <div className="flex flex-col gap-2 border-t border-[var(--border-soft)] pt-3 sm:flex-row sm:items-center sm:justify-between">
        <p className="text-xs text-[var(--text-faint)]">
          保存后计划将载入 Task World 画布，后续执行仍需经过正常审批，不会自动越权运行。
        </p>
        <Button
          type="button"
          disabled={!canConfirm}
          onClick={() => void handleConfirm()}
        >
          <Save className="h-4 w-4" />
          {confirming ? "正在保存图…" : "确认并保存到画布"}
        </Button>
      </div>
    </div>
  );
}
