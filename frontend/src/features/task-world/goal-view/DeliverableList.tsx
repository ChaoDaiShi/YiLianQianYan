import { useState } from "react";
import type { AcceptanceReport, EvidenceSnapshot } from "../../../api/proofloop";
import type { TaskGraphDetail } from "../../../api/taskWorld";
import type { ArtifactRecord } from "../../../api/evidence";
import type { GoalViewState } from "./goalViewModel";

const DELIVERABLE_STATUS: Record<GoalViewState["deliverables"][number]["state"], string> = {
  pass: "已验收",
  fail: "未通过",
  not_checked: "尚未验收",
  human_pending: "等待人工复核",
  stale: "验收已过期",
};

interface DeliverableListProps {
  deliverables: GoalViewState["deliverables"];
  artifacts: ArtifactRecord[];
  snapshot: EvidenceSnapshot | null;
  detail: TaskGraphDetail | null;
  report: AcceptanceReport | null;
  artifactError: string | null;
  onDownload: (artifact: ArtifactRecord) => Promise<void>;
  onViewExecution: (nodeId: string, executionId: string) => void;
}

export default function DeliverableList({
  deliverables,
  artifacts,
  snapshot,
  detail,
  report,
  artifactError,
  onDownload,
  onViewExecution,
}: DeliverableListProps) {
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const artifactById = new Map(artifacts.map((artifact) => [artifact.id, artifact]));

  const download = async (artifact: ArtifactRecord) => {
    setDownloadingId(artifact.id);
    setDownloadError(null);
    try {
      await onDownload(artifact);
    } catch (error: unknown) {
      setDownloadError(error instanceof Error ? error.message : "下载产物失败。");
    } finally {
      setDownloadingId(null);
    }
  };

  return (
    <section className="goal-view-card" id="goal-deliverables" aria-label="目标交付物">
      <div className="goal-view-section-heading">
        <div>
          <h2>交付物</h2>
          <p>仅列出目标契约和当前验收证据中的真实交付物。</p>
        </div>
        <span className="goal-view-count">{deliverables.length}</span>
      </div>
      {artifactError && <p className="goal-view-error" role="alert">产物记录加载失败：{artifactError}</p>}
      {downloadError && <p className="goal-view-error" role="alert">{downloadError}</p>}
      {deliverables.length === 0 ? (
        <p className="goal-view-empty">当前目标没有登记交付物。</p>
      ) : (
        <div className="goal-view-deliverables">
          {deliverables.map((deliverable) => {
            const artifact = deliverable.artifactId ? artifactById.get(deliverable.artifactId) : undefined;
            const origin = artifact
              ? snapshot?.evidence.find((evidence) => evidence.artifact_id === artifact.id)?.origin
                ?? snapshot?.executions.find((execution) => execution.execution_id === artifact.task_execution_id)
              : undefined;
            const sourceNode = origin ? detail?.nodes.find((node) => node.id === origin.node_id) : undefined;
            const acceptance = report?.deliverable_results.find((result) => result.deliverable_id === deliverable.id);

            return (
              <article className="goal-view-deliverable" key={deliverable.id}>
                <div className="goal-view-section-heading">
                  <div className="min-w-0">
                    <h3>{deliverable.title}</h3>
                    <p>{DELIVERABLE_STATUS[deliverable.state]}</p>
                  </div>
                  <span className={`goal-view-phase goal-view-phase-${deliverable.state}`}>{DELIVERABLE_STATUS[deliverable.state]}</span>
                </div>
                {deliverable.reason && <p className="goal-view-deliverable-reason">{deliverable.reason}</p>}
                {artifact ? (
                  <>
                    <p className="goal-view-artifact-meta">
                      {artifact.mime_type || "格式未记录"} · {artifact.size == null ? "大小未记录" : `${artifact.size.toLocaleString()} 字节`} · {artifact.created_at ? new Date(artifact.created_at).toLocaleString() : "时间未记录"}
                    </p>
                    <div className="goal-view-artifact-actions">
                      <button type="button" disabled={downloadingId === artifact.id} onClick={() => void download(artifact)}>
                        {downloadingId === artifact.id ? "正在下载…" : "下载真实文件"}
                      </button>
                      {origin && sourceNode && (
                        <button type="button" onClick={() => onViewExecution(sourceNode.id, origin.execution_id)}>
                          查看执行记录{sourceNode ? ` · ${sourceNode.title}` : ""}
                        </button>
                      )}
                    </div>
                  </>
                ) : deliverable.artifactId ? (
                  <p className="goal-view-artifact-meta">验收记录关联了产物，但当前产物记录不可用{artifactError ? "，请刷新后重试。" : "。"}</p>
                ) : (
                  <p className="goal-view-artifact-meta">当前没有可打开的产物文件。</p>
                )}
                {acceptance?.evidence_ids.length ? <p className="goal-view-artifact-meta">验收证据 {acceptance.evidence_ids.length} 项</p> : null}
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
