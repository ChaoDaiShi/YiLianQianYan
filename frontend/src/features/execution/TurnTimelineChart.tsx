import { useEffect, useState } from "react";
import { Maximize2 } from "lucide-react";
import type { ContextManifestEntry, ConversationTurn } from "../../api/contextManifest";
import type { Message } from "../../types";
import { Button, Modal } from "../../components/ui";
import { projectTimelineTracks, type TimelineRange, type TurnTimelineNode } from "./turnTimeline";
import ExpandedTurnTimeline, { formatTimelineDuration, TimelinePointIcon, type TimelinePoint, type TimelineSpan, type TimelineStage } from "./ExpandedTurnTimeline";
import "./turnTimelineChart.css";

export interface TimelineMemory { at?: number; sources: ContextManifestEntry[] }

const stageLabels: Record<TimelineStage, string> = {
  request: "发送", memory: "记忆", approval: "审批", execution: "执行", verification: "核验", reply: "答复", finish: "结束",
};

function toolStage(phase: TurnTimelineNode["phase"]): TimelineStage {
  if (phase === "request" || phase === "decision") return "approval";
  if (phase === "verification") return "verification";
  return "execution";
}

function trackSpan(range: TimelineRange | undefined, stage: TimelineSpan["stage"]): TimelineSpan[] {
  if (range?.start == null || range.end == null || range.end < range.start) return [];
  return [{ id: range.node.id, stage, start: range.start, end: range.end, live: range.live }];
}

export default function TurnTimelineChart({ turn, nodes, status, runError, requestAt, memory, reply }: {
  turn: ConversationTurn; nodes: TurnTimelineNode[]; status: string; runError?: string | null;
  requestAt: number; memory?: TimelineMemory; reply?: Message;
}) {
  const [open, setOpen] = useState(false);
  const [now, setNow] = useState(Date.now);
  const tracks = projectTimelineTracks(nodes, turn, now);
  const live = tracks.some(track => track.approval?.live || track.execution?.live);
  useEffect(() => {
    if (!live) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(timer);
  }, [live]);

  const sources = memory?.sources ?? [];
  const spans = tracks.flatMap(track => [...trackSpan(track.approval, "approval"), ...trackSpan(track.execution, "execution")]);
  const durations = new Map(spans.map(span => [span.id, span.end - span.start]));
  const totals = (["approval", "execution"] as const).flatMap(stage => {
    const matching = spans.filter(span => span.stage === stage);
    if (!matching.length) return [];
    return [{
      stage,
      label: stage === "approval" ? "审批等待" : "工具执行",
      total: matching.reduce((sum, span) => sum + span.end - span.start, 0),
    }];
  });
  const end = Math.max(turn.finished_at ?? requestAt, memory?.at ?? requestAt, reply?.created_at ?? requestAt,
    ...nodes.map(node => node.at ?? requestAt), ...spans.map(span => span.end));
  const failed = turn.status === "failed" || turn.status === "interrupted";
  const points: TimelinePoint[] = [
    { id: "sent", stage: "request", label: "发送消息", at: requestAt, tone: "neutral" },
    { id: "memory", stage: "memory", label: "记忆引用", at: memory?.at, tone: "neutral",
      detail: sources.length ? `引用 ${sources.length} 项` : memory ? "本轮未引用记忆" : "本轮上下文记录暂不可用" },
    ...nodes.map(node => ({ ...node, stage: toolStage(node.phase), duration: durations.get(node.id) })),
  ];
  if (reply) points.push({ id: "reply", stage: "reply", label: "最终答复", at: reply.created_at, tone: "success" });
  if (turn.finished_at != null) points.push({ id: "finish", stage: "finish", label: status, at: turn.finished_at,
    tone: failed ? "error" : "neutral", reason: runError ?? undefined });
  points.sort((a, b) => (a.at ?? Infinity) - (b.at ?? Infinity));

  const previewStages = new Map(points.map(point => [point.stage, point]));
  const preview = (Object.keys(stageLabels) as TimelineStage[]).flatMap(stage => {
    const point = previewStages.get(stage);
    return point && (stage !== "finish" || !reply || failed) ? [point] : [];
  });
  const focusStart = [...tracks].reverse().find(track => track.execution?.start != null)?.execution?.start;
  const close = () => setOpen(false);
  return <>
    <figure className="turn-timeline-preview" aria-label="本轮时间轴">
      <div className="turn-timeline-preview-head">
        <span>阶段概览</span>
        <Button size="sm" variant="ghost" onClick={() => setOpen(true)}><Maximize2 size={13} />放大查看</Button>
      </div>
      <div className="turn-timeline-preview-flow">
        <div className="turn-timeline-arrow" aria-hidden="true" />
        {preview.map(point => <div className="turn-timeline-preview-step" key={point.stage}>
          <span role="img" aria-label={point.label} title={point.label} className="turn-timeline-preview-node" data-tone={point.tone}>
            <TimelinePointIcon point={point} size={13} />
          </span>
          <span>{stageLabels[point.stage]}</span>
        </div>)}
      </div>
      {totals.length > 0 && <div className="turn-timeline-preview-totals">
        {totals.map(item => <span key={item.stage} role="group" aria-label={item.label}>
          {item.label} <strong data-duration-ms={item.total}>{formatTimelineDuration(item.total)}</strong>
        </span>)}
      </div>}
    </figure>
    <Modal open={open} onClose={close} title="本轮时间轴" size="xl">
      {open && <ExpandedTurnTimeline points={points} spans={spans} origin={requestAt} end={end} focusStart={focusStart}
        sources={sources} reply={reply?.content} onClose={close} />}
    </Modal>
  </>;
}
