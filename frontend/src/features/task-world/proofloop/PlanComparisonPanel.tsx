import { useState } from "react";
import { getProofDraft, selectProofCandidate, type PlanOption, type ProofDraft } from "../../../api/proofloop";
import { Button } from "../../../components/ui";
import { formatFieldPath } from "./proofloopModel";

interface PlanComparisonPanelProps {
  draft: ProofDraft;
  onDraftChange?: (draft: ProofDraft) => void;
  onSelectionIntentChange?: (candidateId: string | null) => void;
}

function percent(value: number): string {
  return `${(value / 100).toFixed(0)}%`;
}

function optionTitle(option: PlanOption, index: number): string {
  const firstStep = option.candidate.nodes[0]?.title;
  return firstStep ? `方案 ${index + 1} · ${firstStep}` : `方案 ${index + 1}`;
}

export default function PlanComparisonPanel({ draft, onDraftChange, onSelectionIntentChange }: PlanComparisonPanelProps) {
  const review = draft.planning_review;
  const [busyId, setBusyId] = useState<string | null>(null);
  const [selectionIntent, setSelectionIntent] = useState<string | null>(null);
  const [needsFreshRevision, setNeedsFreshRevision] = useState(false);
  const [latestRevisionLoaded, setLatestRevisionLoaded] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  if (!review) return null;

  const intentOption = review.options.find((option) => option.candidate_id === selectionIntent);
  const canRetryIntent = Boolean(selectionIntent && intentOption)
    && intentOption?.compilation.status !== "invalid"
    && (!needsFreshRevision || latestRevisionLoaded);

  const saveSelection = async (candidateId: string, revision: number, retry = false) => {
    setBusyId(candidateId);
    setMessage(null);
    try {
      const result = await selectProofCandidate(draft.draft_id, {
        expected_revision: revision,
        candidate_id: candidateId,
      });
      if (result.ok) {
        setSelectionIntent(null);
        setNeedsFreshRevision(false);
        setLatestRevisionLoaded(false);
        onSelectionIntentChange?.(null);
        onDraftChange?.(result.data);
        return;
      }

      if (result.error.status === 409) {
        setSelectionIntent(candidateId);
        setNeedsFreshRevision(true);
        setLatestRevisionLoaded(false);
        const latest = await getProofDraft(draft.draft_id);
        if (latest.ok) {
          onDraftChange?.(latest.data);
          setLatestRevisionLoaded(true);
          setMessage(`草稿版本已变化，已加载最新版本。${retry ? "请确认后按最新版本重新选择此方案。" : "你的选择意图已保留，请确认后重新选择。"}`);
        } else {
          setMessage(`草稿版本已变化，读取最新版本失败：${latest.error.message}。你的方案选择意图仍保留。`);
        }
        return;
      }

      setNeedsFreshRevision(false);
      setLatestRevisionLoaded(false);
      setMessage(result.error.message || "保存方案选择失败。");
    } catch (error: unknown) {
      setNeedsFreshRevision(false);
      setLatestRevisionLoaded(false);
      setMessage(error instanceof Error ? error.message : "保存方案选择失败。");
    } finally {
      setBusyId(null);
    }
  };

  const retrySelection = () => {
    if (!selectionIntent || !canRetryIntent) return;
    void saveSelection(selectionIntent, draft.revision, true);
  };

  const reloadLatestForIntent = async () => {
    if (!selectionIntent) return;
    setBusyId(selectionIntent);
    try {
      const latest = await getProofDraft(draft.draft_id);
      if (latest.ok) {
        onDraftChange?.(latest.data);
        setLatestRevisionLoaded(true);
        setMessage("已加载最新草稿版本。请确认后重新保存保留的方案选择。");
      } else {
        setMessage(`读取最新草稿失败：${latest.error.message}。方案选择意图仍保留。`);
      }
    } catch (error: unknown) {
      setMessage(error instanceof Error ? error.message : "读取最新草稿失败。");
    } finally {
      setBusyId(null);
    }
  };

  return (
    <section className="flex flex-col gap-3 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-elevated)] p-3" aria-label="方案比较">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <h3 className="text-sm font-semibold text-[var(--text)]">方案比较</h3>
          <p className="mt-1 text-xs text-[var(--text-secondary)]">
            {review.strategy === "experience" ? "已审核经验增强" : review.strategy === "ranked" ? "按能力匹配排序" : "原有规划"}
            {review.recommendations.length > 0 ? ` · ${review.recommendations.length} 条已审核经验` : " · 当前没有可用的已审核经验"}
          </p>
        </div>
        <span className="text-[11px] text-[var(--text-faint)]">推荐方案仍需由你明确选择</span>
      </div>
      {review.recommendations.length > 0 && (
        <div className="rounded border border-[var(--border-soft)] bg-[var(--surface)] p-2 text-xs">
          <strong className="text-[var(--text)]">本次参考的已审核经验</strong>
          <ul className="mt-1 grid gap-1 text-[var(--text-secondary)]">
            {review.recommendations.map((recommendation) => (
              <li key={`${recommendation.skill_name}:${recommendation.version}`}>
                <span className="font-medium">{recommendation.skill_name} v{recommendation.version}</span>
                {recommendation.rationale && <span> · {recommendation.rationale}</span>}
              </li>
            ))}
          </ul>
        </div>
      )}

      {message && <p className="rounded border border-[var(--warning-border)] bg-[var(--warning-soft)] p-2 text-xs text-[var(--warning-fg)]" role="status">{message}</p>}
      {selectionIntent && (
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <span className="text-[var(--text-secondary)]">保留的选择：{intentOption ? optionTitle(intentOption, review.options.indexOf(intentOption)) : needsFreshRevision ? "该方案已不在最新草稿中" : "等待保存结果"}</span>
          {canRetryIntent && <Button size="sm" variant="secondary" disabled={busyId !== null} onClick={retrySelection}>{needsFreshRevision ? "按最新版本重新选择此方案" : "重试保存此方案"}</Button>}
          {needsFreshRevision && !latestRevisionLoaded && <Button size="sm" variant="secondary" disabled={busyId !== null} onClick={() => void reloadLatestForIntent()}>重新读取最新草稿</Button>}
        </div>
      )}

      {review.options.length === 0 ? (
        <p className="rounded border border-[var(--warning-border)] p-3 text-xs text-[var(--warning-fg)]">后端未返回候选方案。请检查草稿错误信息后调整目标。</p>
      ) : (
        <div className="grid gap-3 xl:grid-cols-2">
          {review.options.map((option, index) => {
            const selected = review.selected_candidate_id === option.candidate_id;
            const recommended = review.recommended_candidate_id === option.candidate_id;
            const invalid = option.compilation.status === "invalid";
            const title = optionTitle(option, index);
            const coveredDeliverables = option.candidate.deliverable_nodes
              .map((mapping) => draft.contract.deliverables.find((item) => item.id === mapping.deliverable_id)?.title || mapping.deliverable_id);

            return (
              <article key={option.candidate_id} className="flex min-w-0 flex-col gap-3 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface)] p-3" aria-label={title}>
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <h4 className="truncate text-sm font-semibold text-[var(--text)]">{title}</h4>
                    <div className="mt-1 flex flex-wrap gap-1.5">
                      {recommended && <span className="rounded-full bg-[var(--accent-soft)] px-2 py-0.5 text-[11px] text-[var(--accent-soft-fg)]">推荐方案</span>}
                      {selected && <span className="rounded-full bg-[var(--success-soft)] px-2 py-0.5 text-[11px] text-[var(--success-fg)]">已选方案</span>}
                      <span className="rounded-full bg-[var(--surface-muted)] px-2 py-0.5 text-[11px] text-[var(--text-secondary)]">
                        {option.compilation.status === "ready" ? "可确认" : option.compilation.status === "needs_input" ? "需要补充输入" : "校验未通过"}
                      </span>
                    </div>
                  </div>
                  <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    aria-label={`选择方案 ${index + 1}`}
                    aria-pressed={selected}
                    disabled={invalid || selected || busyId !== null}
                    onClick={() => {
                      setSelectionIntent(option.candidate_id);
                      setNeedsFreshRevision(false);
                      setLatestRevisionLoaded(false);
                      onSelectionIntentChange?.(option.candidate_id);
                      void saveSelection(option.candidate_id, draft.revision);
                    }}
                  >
                    {busyId === option.candidate_id ? "保存中…" : selected ? "当前已选" : invalid ? "不可选择" : "选择此方案"}
                  </Button>
                </div>

                <dl className="grid grid-cols-2 gap-x-3 gap-y-1 text-xs">
                  <div><dt className="text-[var(--text-faint)]">交付物覆盖</dt><dd className="text-[var(--text)]">{coveredDeliverables.length ? coveredDeliverables.join("、") : "未映射目标交付物"}</dd></div>
                  <div><dt className="text-[var(--text-faint)]">步骤数</dt><dd className="text-[var(--text)]">{option.candidate.nodes.length}</dd></div>
                  <div><dt className="text-[var(--text-faint)]">覆盖评分</dt><dd className="text-[var(--text)]">{percent(option.score.coverage_bp)}</dd></div>
                  <div><dt className="text-[var(--text-faint)]">风险评分</dt><dd className="text-[var(--text)]">{percent(option.score.risk_bp)}</dd></div>
                  <div><dt className="text-[var(--text-faint)]">综合评分</dt><dd className="text-[var(--text)]">{option.score.total}</dd></div>
                  <div className="col-span-2"><dt className="text-[var(--text-faint)]">预计成本</dt><dd className="text-[var(--text)]">{option.score.cost_known ? `可评估 · 评分 ${percent(option.score.cost_bp)}` : "预计成本未知"}</dd></div>
                </dl>

                {option.compilation.pending_inputs.length > 0 && (
                  <div className="rounded border border-[var(--warning-border)] bg-[var(--warning-soft)] p-2 text-xs">
                    <strong className="text-[var(--warning-fg)]">缺失输入</strong>
                    <ul className="mt-1 list-disc pl-4 text-[var(--text-secondary)]">
                      {option.compilation.pending_inputs.map((input, inputIndex) => (
                        <li key={`${input.node_id}:${inputIndex}`}>{option.candidate.nodes.find((node) => node.id === input.node_id)?.title || input.node_id} · {formatFieldPath(input.target_path)}</li>
                      ))}
                    </ul>
                  </div>
                )}
                {option.compilation.diagnostics.length > 0 && (
                  <div className="rounded border border-[var(--danger-border)] bg-[var(--danger-soft)] p-2 text-xs" aria-label="方案诊断">
                    <strong className="text-[var(--danger-fg)]">编译诊断</strong>
                    <ul className="mt-1 list-disc pl-4 text-[var(--text-secondary)]">
                      {option.compilation.diagnostics.map((diagnostic, diagnosticIndex) => <li key={`${diagnostic.code}:${diagnosticIndex}`}>{diagnostic.message || diagnostic.code}</li>)}
                    </ul>
                  </div>
                )}
                {option.score.rejected_codes.length > 0 && (
                  <p className="text-xs text-[var(--warning-fg)]">排除原因：{option.score.rejected_codes.join("、")}</p>
                )}

                <details className="rounded border border-[var(--border-soft)] p-2">
                  <summary className="cursor-pointer text-xs font-medium text-[var(--text-secondary)]">查看每一步的工具与参数</summary>
                  <ol className="mt-2 grid gap-2">
                    {option.candidate.nodes.map((node, nodeIndex) => (
                      <li key={node.id} className="rounded bg-[var(--surface-muted)] p-2 text-xs">
                        <strong>{nodeIndex + 1}. {node.title}</strong>
                        <p className="mt-1 break-words text-[var(--text-secondary)]">工具：{node.capability_id}</p>
                        {node.instruction && <p className="mt-1 break-words text-[var(--text-secondary)]">{node.instruction}</p>}
                        <pre className="mt-1 max-h-36 overflow-auto whitespace-pre-wrap break-all text-[11px]">参数：{JSON.stringify(node.literal_input, null, 2)}</pre>
                        {node.bindings.map((binding, bindingIndex) => (
                          <p key={`${binding.source.node_id}:${bindingIndex}`} className="mt-1 break-words text-[var(--text-faint)]">{formatFieldPath(binding.target_path)} ← {binding.source.node_id}.{formatFieldPath(binding.source.path)}</p>
                        ))}
                      </li>
                    ))}
                  </ol>
                </details>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
