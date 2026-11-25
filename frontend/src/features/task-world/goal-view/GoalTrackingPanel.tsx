import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useGoalTracking } from "../hooks/useGoalTracking";
import { Modal } from "../../../components/ui";
import { executeCommand } from "../../../api/commands";
import { downloadArtifact, type ArtifactRecord } from "../../../api/evidence";
import GoalOverview from "./GoalOverview";
import GoalTrackingSummary, { goalNodeDestination } from "./GoalTrackingSummary";
import GoalTaskControls, { type GoalControlCommand } from "./GoalTaskControls";

export default function GoalTrackingPanel({ graphId, compact = false }: { graphId: string; compact?: boolean }) {
  const { data, loading, error, refresh, connection, eventError } = useGoalTracking(graphId);
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  async function command(name: GoalControlCommand) {
    setBusy(true); setActionError(null);
    try {
      const result = await executeCommand(name, { graph_id: graphId });
      if (!result || result.status !== "succeeded") throw new Error(result?.error?.message || "操作未成功，请刷新后重试。");
      await refresh(false, true);
    } catch (cause) { setActionError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setBusy(false); }
  }
  async function download(artifact: ArtifactRecord) {
    const blob = await downloadArtifact(artifact.id);
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a"); link.href = url; link.download = artifact.name; link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  const overview = data && <GoalOverview graphId={graphId} contract={data.binding?.contract ?? null} detail={data.detail} report={data.report} snapshot={data.snapshot} currentEvidence={data.currentEvidence} conversations={data.binding?.conversations} artifacts={data.artifacts} artifactError={data.artifactError} proofError={error} proofLoading={loading} onDownloadArtifact={download} onViewExecution={nodeId => { setOpen(false); navigate(goalNodeDestination(graphId, nodeId)); }} />;
  return <div className="goal-tracking-panel" data-compact={compact}>
    {error && <p className="goal-tracking-notice text-[var(--danger)]" role="alert">状态同步失败：{error}。{data && "下方为上次读取的记录。"}<button type="button" onClick={() => void refresh()}>重新读取</button></p>}
    {!data ? !error && <p className="goal-tracking-notice" role="status">正在读取目标与执行记录…</p> : <>
      <GoalTrackingSummary data={data} onDetails={compact ? () => setOpen(true) : undefined}>
        <GoalTaskControls data={data} error={error} busy={busy} loading={loading} onRefresh={refresh} onControl={command} />
        {data.binding?.conversation_ids?.map(id => <Link key={id} to={`/chat/${encodeURIComponent(id)}`}>任务对话</Link>)}
      </GoalTrackingSummary>
      {connection === "reconnecting" && <p className="goal-tracking-notice" role="status">实时连接正在恢复，仍会定时核对任务状态。</p>}
      {connection === "unavailable" && <p className="goal-tracking-notice" role="status">实时订阅不可用：{eventError}</p>}
      {actionError && <p className="goal-tracking-notice text-[var(--danger)]" role="alert">{actionError}</p>}
      {!compact && overview}
      <Modal open={compact && open} onClose={() => setOpen(false)} title="任务详情与成果" size="xl"><div className="goal-tracking-modal">{overview}</div></Modal>
    </>}
  </div>;
}
