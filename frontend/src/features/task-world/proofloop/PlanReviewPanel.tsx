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
  updateProofDraft,
  type ConfirmProofDraftResponse,
  type FieldPath,
  type PendingInput,
  type ProofDraft,
} from "../../../api/proofloop";
import { Badge, Button, Input } from "../../../components/ui";
import {
  formatDiagnosticMessage,
  formatDraftStatus,
  formatFieldPath,
  formatPendingInputReason,
  isDraftSubmittable,
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

  const statusMeta = formatDraftStatus(draft.status);
  const canConfirm = isDraftSubmittable(draft) && !confirming && !updating;

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

      const updatedCandidate = {
        ...draft.candidate,
        nodes: draft.candidate.nodes.map((n) => {
          if (n.id !== pending.node_id) return n;
          const nextLiteral = { ...n.literal_input };
          setDeepValue(nextLiteral, pending.target_path, parsedValue);
          return { ...n, literal_input: nextLiteral };
        }),
      };

      const result = await updateProofDraft(draft.draft_id, {
        expected_revision: draft.revision,
        contract: draft.contract,
        candidate: updatedCandidate,
        binding_confirmations: draft.binding_confirmations,
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
        </div>
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
        <div className="flex flex-col gap-2 rounded-[var(--radius-md)] border border-[var(--accent-soft-border,var(--border-soft))] bg-[var(--surface-elevated)] p-3 text-xs">
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
                      节点 [{pending.node_id}] → 参数 [{formatFieldPath(pending.target_path)}]
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
                    {i + 1}. {node.title} <span className="font-normal text-[var(--text-faint)]">({node.id})</span>
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
                        [{formatFieldPath(b.target_path)}] ← {b.source.node_id}.[{formatFieldPath(b.source.path)}]
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

function setDeepValue(obj: Record<string, unknown>, path: FieldPath, value: unknown) {
  if (path.length === 0) return;
  let curr = obj;
  for (let i = 0; i < path.length - 1; i++) {
    const key = path[i];
    if (typeof curr[key] !== "object" || curr[key] === null) {
      curr[key] = {};
    }
    curr = curr[key] as Record<string, unknown>;
  }
  curr[path[path.length - 1]] = value;
}
