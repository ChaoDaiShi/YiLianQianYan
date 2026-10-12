import { useState } from "react";
import {
  CheckCircle,
  ExternalLink,
  FileCheck,
  HelpCircle,
  Link,
  RefreshCw,
  UserCheck,
  Wrench,
  XCircle,
} from "lucide-react";
import {
  submitHumanReview,
  type AcceptanceReport,
  type EvidenceRef,
  type EvidenceSnapshot,
} from "../../../api/proofloop";
import { Badge, Button, Input } from "../../../components/ui";
import {
  formatCriterionStatus,
  formatGoalOverallStatus,
} from "./proofloopModel";

export interface AcceptancePanelProps {
  goalId: string;
  report: AcceptanceReport | null;
  evidenceSnapshot?: EvidenceSnapshot | null;
  onRefreshEvaluation?: () => void;
  onRequestRepair?: (report: AcceptanceReport) => void;
  onOpenArtifact?: (artifactId: string) => void;
  className?: string;
}

export default function AcceptancePanel({
  goalId,
  report,
  evidenceSnapshot,
  onRefreshEvaluation,
  onRequestRepair,
  onOpenArtifact,
  className = "",
}: AcceptancePanelProps) {
  const [submittingReview, setSubmittingReview] = useState(false);
  const [reviewDecision, setReviewDecision] = useState<"pass" | "fail">("pass");
  const [reviewReason, setReviewReason] = useState("");
  const [activeReviewCriterionId, setActiveReviewCriterionId] = useState<string | null>(null);
  const [reviewError, setReviewError] = useState<string | null>(null);

  const overallMeta = report ? formatGoalOverallStatus(report.status) : null;

  const handleSubmitReview = async (criterionId: string) => {
    if (!report) return;
    setSubmittingReview(true);
    setReviewError(null);

    try {
      const result = await submitHumanReview(report.report_id, {
        criterion_id: criterionId,
        decision: reviewDecision,
        reason: reviewReason.trim() || (reviewDecision === "pass" ? "人工审核确认通过" : "人工审核判定未达标"),
      });

      if (result.ok) {
        setActiveReviewCriterionId(null);
        setReviewReason("");
        onRefreshEvaluation?.();
      } else {
        setReviewError(result.error.message || "提交审核决定失败。");
      }
    } catch (err: unknown) {
      setReviewError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmittingReview(false);
    }
  };

  if (!report) {
    return (
      <div className={`acceptance-panel rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface)] p-4 text-xs text-[var(--text-faint)] ${className}`}>
        <p>暂无目标验收报告。待任务执行完成后，系统将自动汇总成果证据并进行验收。</p>
        {onRefreshEvaluation && (
          <Button size="sm" variant="secondary" className="mt-2" onClick={onRefreshEvaluation}>
            <RefreshCw className="h-3.5 w-3.5" /> 立即检查验收
          </Button>
        )}
      </div>
    );
  }

  const findEvidence = (evidenceId: string): EvidenceRef | undefined => {
    return evidenceSnapshot?.evidence.find((e) => e.evidence_id === evidenceId);
  };

  return (
    <div className={`acceptance-panel flex flex-col gap-4 rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface)] p-4 ${className}`}>
      {/* Header and overall outcome */}
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--border-soft)] pb-3">
        <div>
          <div className="flex items-center gap-2">
            <FileCheck className="h-5 w-5 text-[var(--accent)]" />
            <h2 className="text-base font-semibold text-[var(--text)]">目标成果验收报告</h2>
            {overallMeta && <Badge tone={overallMeta.tone}>{overallMeta.label}</Badge>}
          </div>
          {overallMeta && (
            <p className="mt-1 text-xs text-[var(--text-faint)]">{overallMeta.description}</p>
          )}
        </div>
        <div className="flex items-center gap-2">
          {onRefreshEvaluation && (
            <Button size="sm" variant="ghost" onClick={onRefreshEvaluation}>
              <RefreshCw className="h-3.5 w-3.5" />
              刷新
            </Button>
          )}
          {report.status !== "pass" && onRequestRepair && (
            <Button size="sm" variant="secondary" onClick={() => onRequestRepair(report)}>
              <Wrench className="h-3.5 w-3.5" />
              查看修复建议
            </Button>
          )}
        </div>
      </div>

      <div className="text-xs text-[var(--text-faint)]">
        目标 ID: {goalId} · 报告 ID: {report.report_id} · 目标版本: v{report.goal_version} · 图版本: r{report.graph_revision}
      </div>

      {/* Criteria checks list */}
      <div className="flex flex-col gap-2">
        <h3 className="text-xs font-semibold text-[var(--text)]">
          准则条件核对详情（共 {report.results.length} 项）
        </h3>
        <div className="flex flex-col gap-2">
          {report.results.map((item) => {
            const statusMeta = formatCriterionStatus(item.status);
            const isReviewing = activeReviewCriterionId === item.criterion_id;

            return (
              <div
                key={item.criterion_id}
                className="flex flex-col gap-1.5 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-elevated)] p-3 text-xs"
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-1.5">
                    {item.status === "pass" && <CheckCircle className="h-4 w-4 text-[var(--success)]" />}
                    {item.status === "fail" && <XCircle className="h-4 w-4 text-[var(--danger)]" />}
                    {item.status === "human_pending" && <UserCheck className="h-4 w-4 text-[var(--warning-fg)]" />}
                    {item.status === "not_checked" && <HelpCircle className="h-4 w-4 text-[var(--text-faint)]" />}
                    <span className="font-medium text-[var(--text)]">准则 {item.criterion_id}</span>
                  </div>
                  <Badge tone={statusMeta.tone}>{statusMeta.label}</Badge>
                </div>

                <p className="text-[var(--text-secondary)]">{item.reason || "暂无具体检查说明"}</p>

                {/* Evidence links */}
                {item.evidence_ids.length > 0 && (
                  <div className="mt-1 flex flex-col gap-1 rounded border border-[var(--border-soft)] bg-[var(--surface)] p-2 text-[11px]">
                    <span className="font-medium text-[var(--text-faint)]">支持证据：</span>
                    {item.evidence_ids.map((eid) => {
                      const ev = findEvidence(eid);
                      return (
                        <div key={eid} className="flex flex-wrap items-center justify-between gap-1 text-[var(--text-secondary)]">
                          <span>
                            证据 [{eid.slice(0, 8)}] · 摘要: {ev?.digest ? ev.digest.slice(0, 16) + "…" : "待计算"}
                          </span>
                          {ev?.artifact_id && onOpenArtifact && (
                            <button
                              type="button"
                              onClick={() => onOpenArtifact(ev.artifact_id!)}
                              className="inline-flex items-center gap-0.5 text-[var(--accent)] hover:underline"
                            >
                              打开产物 <ExternalLink size={10} />
                            </button>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}

                {/* Human review form if pending */}
                {item.status === "human_pending" && (
                  <div className="mt-2 rounded border border-[var(--warning-border)] bg-[var(--warning-soft)] p-2">
                    {!isReviewing ? (
                      <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => {
                          setActiveReviewCriterionId(item.criterion_id);
                          setReviewReason("");
                        }}
                      >
                        进行人工审核判断
                      </Button>
                    ) : (
                      <div className="flex flex-col gap-2">
                        <span className="font-medium text-[var(--text)]">人工审核结论：</span>
                        <div className="flex gap-4">
                          <label className="flex items-center gap-1">
                            <input
                              type="radio"
                              name={`decision_${item.criterion_id}`}
                              checked={reviewDecision === "pass"}
                              onChange={() => setReviewDecision("pass")}
                            />
                            <span>确认通过</span>
                          </label>
                          <label className="flex items-center gap-1">
                            <input
                              type="radio"
                              name={`decision_${item.criterion_id}`}
                              checked={reviewDecision === "fail"}
                              onChange={() => setReviewDecision("fail")}
                            />
                            <span>判定未通过</span>
                          </label>
                        </div>
                        <Input
                          placeholder="填写审核依据或说明（可选）"
                          value={reviewReason}
                          onChange={(e) => setReviewReason(e.target.value)}
                        />
                        <div className="flex justify-end gap-2">
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => setActiveReviewCriterionId(null)}
                          >
                            取消
                          </Button>
                          <Button
                            size="sm"
                            disabled={submittingReview}
                            onClick={() => void handleSubmitReview(item.criterion_id)}
                          >
                            {submittingReview ? "提交中…" : "提交审核结论"}
                          </Button>
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {reviewError && (
        <div className="rounded-[var(--radius-md)] border border-[var(--danger-border)] bg-[var(--danger-soft)] p-2 text-xs text-[var(--danger-fg)]" role="alert">
          {reviewError}
        </div>
      )}

      {/* Sources Traceability if present in snapshot */}
      {evidenceSnapshot?.sources && evidenceSnapshot.sources.length > 0 && (
        <div className="rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-elevated)] p-3 text-xs">
          <div className="flex items-center gap-1.5 font-medium text-[var(--text)]">
            <Link className="h-4 w-4 text-[var(--accent)]" />
            <span>查证来源追溯（{evidenceSnapshot.sources.length} 条）</span>
          </div>
          <ul className="mt-2 list-disc pl-4 text-[var(--text-secondary)]">
            {evidenceSnapshot.sources.map((src) => (
              <li key={src.source_id}>
                <span className="font-mono">{src.uri}</span>
                {src.cited_by_deliverable_ids.length > 0 && (
                  <span className="ml-1 text-[var(--text-faint)]">
                    （引用交付物: {src.cited_by_deliverable_ids.join(", ")}）
                  </span>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
