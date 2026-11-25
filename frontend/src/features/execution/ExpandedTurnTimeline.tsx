import { useEffect, useRef, useState } from "react";
import { AlertCircle, BookOpen, Check, ChevronLeft, ChevronRight, Clock3, Flag, MessageSquareText, Minus, Play, Plus, Send, ShieldCheck, ZoomIn } from "lucide-react";
import { Link } from "react-router-dom";
import type { ContextManifestEntry } from "../../api/contextManifest";
import { presentExecutionError } from "../../components/chat/errorDisplay";
import ConversationMarkdown from "../../components/chat/ConversationMarkdown";
import { Button } from "../../components/ui";
import { groupTimelinePoints, layoutTimelineLabels, timelineViewport, type TimelineTone } from "./turnTimeline";

export type TimelineStage = "request" | "memory" | "approval" | "execution" | "verification" | "reply" | "finish";

export interface TimelinePoint {
  id: string;
  stage: TimelineStage;
  label: string;
  tone: TimelineTone;
  at?: number;
  duration?: number;
  detail?: string;
  reason?: string;
}

export interface TimelineSpan { id: string; stage: "approval" | "execution"; start: number; end: number; live: boolean }

const stageIcons = { request: Send, memory: BookOpen, approval: ShieldCheck, execution: Play, verification: Check, reply: MessageSquareText, finish: Flag };
const seconds = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 3 });
const clock = new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit", hourCycle: "h23" });
const fullView = { zoom: 1, center: 0, expanded: [] as string[] };

export function formatTimelineDuration(ms: number) {
  const rounded = Math.round(ms);
  if (rounded < 1000) return `${rounded} 毫秒`;
  if (rounded < 60000) return `${seconds.format(rounded / 1000)} 秒`;
  return `${Math.floor(rounded / 60000)} 分 ${Math.floor(rounded / 1000) % 60} 秒`;
}

export function TimelinePointIcon({ point, size = 15 }: { point: TimelinePoint; size?: number }) {
  const Icon = point.tone === "error" ? AlertCircle : point.tone === "waiting" ? Clock3 : stageIcons[point.stage];
  return <Icon size={size} aria-hidden="true" />;
}

function memoryHref(source: ContextManifestEntry) {
  return source.kind === "prior_chat" ? `/chat/${encodeURIComponent(source.source_id)}` : `/memory?record=${encodeURIComponent(source.source_id)}`;
}

function TimelinePointDetails({ point, origin, sources, reply, onClose }: {
  point: TimelinePoint; origin: number; sources: ContextManifestEntry[]; reply?: string; onClose: () => void;
}) {
  const time = point.at == null ? undefined : new Date(point.at);
  const error = point.reason ? presentExecutionError(point.reason) : null;
  return <section aria-label="节点详情" className="turn-timeline-inspector">
    <h4 className="turn-timeline-inspector-title" data-tone={point.tone}><TimelinePointIcon point={point} />{point.label}</h4>
    <p className="turn-timeline-inspector-time">
      {time ? <><time dateTime={time.toISOString()}>{clock.format(time)}.{String(time.getMilliseconds()).padStart(3, "0")}</time>
        <span>+{formatTimelineDuration(time.getTime() - origin)}</span></> : <span>时间未记录</span>}
      {point.duration != null && <span>耗时 {formatTimelineDuration(point.duration)}</span>}
    </p>
    {point.detail && <p className="break-words text-xs text-[var(--text-secondary)]">{point.detail}</p>}
    {point.stage === "memory" && sources.length > 0 && <ul className="mt-2 space-y-1">
      {sources.map((source, index) => <li key={`${source.kind}:${source.source_id}:${source.version ?? ""}`}>
        <Link to={memoryHref(source)} onClick={onClose} className="text-[var(--accent-soft-fg)] underline">
          {source.kind === "prior_chat" ? "打开来源对话" : "查看引用记忆"}{sources.length > 1 ? ` ${index + 1}` : ""}
        </Link>
      </li>)}
    </ul>}
    {point.stage === "reply" && reply && <div className="mt-2"><ConversationMarkdown text={reply} /></div>}
    {point.tone === "error" && <div className="mt-2 whitespace-pre-wrap break-words text-sm">
      <p>{error?.message ?? "该节点没有记录具体原因。"}</p>
      {error && error.technical !== error.message && <p className="mt-1 text-xs text-[var(--text-secondary)]">{error.technical}</p>}
    </div>}
  </section>;
}

export default function ExpandedTurnTimeline({ points, spans, origin, end, focusStart, sources, reply, onClose }: {
  points: TimelinePoint[]; spans: TimelineSpan[]; origin: number; end: number; focusStart?: number;
  sources: ContextManifestEntry[]; reply?: string; onClose: () => void;
}) {
  const [selection, setSelection] = useState(() => [...points].reverse().find(point => point.stage === "execution")?.id ?? points[0].id);
  const [view, setView] = useState(fullView);
  const plotRef = useRef<HTMLDivElement>(null);
  const focusPoint = useRef<string | null>(null);
  const [plotWidth, setPlotWidth] = useState(750);
  useEffect(() => {
    const plot = plotRef.current;
    if (!plot) return;
    const observer = new ResizeObserver(([entry]) => setPlotWidth(entry.contentRect.width));
    observer.observe(plot);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (!focusPoint.current) return;
    const buttons = plotRef.current?.querySelectorAll<HTMLButtonElement>("button[data-point-id]");
    Array.from(buttons ?? []).find(button => button.dataset.pointId === focusPoint.current)?.focus({ preventScroll: true });
    focusPoint.current = null;
  }, [view.expanded]);

  const selected = points.find(point => point.id === selection) ?? points[0];
  const extent = Math.max(1, end - origin);
  const maxZoom = Math.max(1, extent / 20);
  const zoom = Math.min(view.zoom, maxZoom);
  const window = timelineViewport(extent, zoom, view.center);
  const width = window.end - window.start;
  const midpoint = window.start + width / 2;
  const visibleStart = origin + window.start;
  const visibleEnd = origin + window.end;
  const offset = (time: number) => (time - visibleStart) / width * 100;
  const expanded = new Set(view.expanded);
  const visible = points.flatMap(point => point.at != null && point.at >= visibleStart && point.at <= visibleEnd
    ? [{ point, at: point.at, x: offset(point.at), expanded: expanded.has(point.id) }] : []);
  const groups = groupTimelinePoints(visible, 60 / Math.max(1, plotWidth) * 100).map(items => ({
    items, x: (items[0].x + items[items.length - 1].x) / 2,
  }));
  const untimed = points.filter(point => point.at == null);
  const labelMargin = Math.min(45, 57 / Math.max(1, plotWidth) * 100);
  const labelPositions = groups.map(({ x }) => Math.max(labelMargin, Math.min(100 - labelMargin, x)));
  const lanes = layoutTimelineLabels(labelPositions, 120 / Math.max(1, plotWidth) * 100);
  const above = Math.max(1, ...lanes.filter(lane => lane % 2 === 0).map(lane => lane / 2 + 1));
  const below = Math.max(0, ...lanes.filter(lane => lane % 2 !== 0).map(lane => (lane + 1) / 2));
  const axisY = 20 + above * 43;

  function changeZoom(factor: number) {
    const at = selected.at;
    const center = at != null && at >= visibleStart && at <= visibleEnd ? at - origin : midpoint;
    const nextZoom = Math.max(1, Math.min(maxZoom, zoom * factor));
    setView(nextZoom === 1 ? fullView : { ...view, zoom: nextZoom, center });
  }

  function zoomToRange(start: number, finish: number, expanded: string[]) {
    const focusWidth = Math.max(20, (finish - start) * 1.26);
    setView({ zoom: Math.max(1, extent / focusWidth), center: (start + finish) / 2 - origin, expanded });
  }

  function focusExecution() {
    if (focusStart == null) return;
    zoomToRange(focusStart, end, points.filter(point => point.at != null && point.at >= focusStart).map(point => point.id));
  }

  return <>
    <div className="turn-timeline-toolbar">
      <div className="turn-timeline-controls" role="group" aria-label="缩放与定位">
        <Button size="sm" variant="secondary" aria-label="缩小时间轴" title="缩小" disabled={zoom <= 1} onClick={() => changeZoom(0.5)}><Minus size={15} /></Button>
        <span className="turn-timeline-zoom" aria-label="缩放倍数">{seconds.format(zoom)}×</span>
        <Button size="sm" variant="secondary" aria-label="放大时间轴" title="放大" disabled={zoom >= maxZoom} onClick={() => changeZoom(2)}><Plus size={15} /></Button>
        <Button size="sm" variant="ghost" onClick={() => setView(fullView)}>全程</Button>
        {focusStart != null && <Button size="sm" variant="secondary" onClick={focusExecution}>聚焦执行</Button>}
      </div>
      <div className="turn-timeline-controls" role="group" aria-label="移动时间窗口">
        <Button size="sm" variant="ghost" aria-label="查看更早" disabled={window.start <= 0} onClick={() => setView({ ...view, zoom, center: midpoint - width / 2 })}><ChevronLeft size={16} /></Button>
        <span className="turn-timeline-window">{formatTimelineDuration(window.start)} – {formatTimelineDuration(window.end)}</span>
        <Button size="sm" variant="ghost" aria-label="查看更晚" disabled={window.end >= extent - 0.5} onClick={() => setView({ ...view, zoom, center: midpoint + width / 2 })}><ChevronRight size={16} /></Button>
      </div>
    </div>
    <figure className="turn-timeline-expanded" aria-label="可缩放时间轴" data-window-ms={width}>
      <div ref={plotRef} className="turn-timeline-expanded-plot" style={{ height: axisY + below * 43 + 39 }}>
        <div className="turn-timeline-arrow" style={{ top: axisY }} aria-hidden="true" />
        {[0, 25, 50, 75, 100].map(percent => <div key={percent} className="turn-timeline-tick" style={{ left: `${percent}%`, top: axisY }} aria-hidden="true">
          <span>{formatTimelineDuration(window.start + width * percent / 100)}</span>
        </div>)}
        {spans.filter(span => span.end >= visibleStart && span.start <= visibleEnd).map(span => {
          const left = offset(Math.max(span.start, visibleStart));
          return <span key={span.id} className="turn-timeline-span" data-stage={span.stage} data-live={span.live || undefined}
            style={{ top: axisY, left: `${left}%`, width: `${offset(Math.min(span.end, visibleEnd)) - left}%` }} aria-hidden="true" />;
        })}
        {groups.map(({ items, x }, index) => {
          const first = items[0];
          const last = items[items.length - 1];
          const point = first.point;
          const grouped = items.length > 1;
          const tone = grouped ? items.some(item => item.point.tone === "error") ? "error" : "neutral" : point.tone;
          const lane = lanes[index];
          const distance = 43 * (Math.floor(lane / 2) + 1);
          const y = axisY + (lane % 2 === 0 ? -distance : distance);
          const labelX = labelPositions[index];
          return <div className="turn-timeline-point" key={point.id} data-tone={tone}>
            <span className="turn-timeline-stem" style={{ left: `${x}%`, top: Math.min(y, axisY), height: distance }} aria-hidden="true" />
            {labelX !== x && <span className="turn-timeline-connector" style={{ left: `${Math.min(x, labelX)}%`, top: y, width: `${Math.abs(labelX - x)}%` }} aria-hidden="true" />}
            <span className="turn-timeline-dot" style={{ left: `${x}%`, top: axisY }} aria-hidden="true" />
            {grouped ? <button type="button" className="turn-timeline-event turn-timeline-cluster" aria-label={`放大 ${items.length} 个节点`}
              title={items.map(item => item.point.label).join(" · ")} data-node-count={items.length}
              style={{ left: `${labelX}%`, top: y }} onClick={() => {
                const ids = items.map(item => item.point.id);
                const next = ids.includes(selected.id) ? selected.id : point.id;
                focusPoint.current = next;
                setSelection(next);
                zoomToRange(first.at, last.at, ids);
              }}>
              <span><ZoomIn size={13} aria-hidden="true" />{items.length} 个节点</span>
              <small>{formatTimelineDuration(first.at - origin)} – {formatTimelineDuration(last.at - origin)}</small>
            </button> : <button type="button" className="turn-timeline-event" aria-label={point.label} aria-pressed={point.id === selected.id}
              data-at={first.at} data-point-id={point.id} style={{ left: `${labelX}%`, top: y }} onClick={() => setSelection(point.id)}>
              <span><TimelinePointIcon point={point} size={13} />{point.label}</span>
              <small>+{formatTimelineDuration(first.at - origin)}</small>
            </button>}
          </div>;
        })}
      </div>
      {untimed.length > 0 && <div className="turn-timeline-untimed">
        <span>时间未记录</span>
        {untimed.map(point => <Button key={point.id} size="sm" variant="ghost" aria-pressed={point.id === selected.id} onClick={() => setSelection(point.id)}>{point.label}</Button>)}
      </div>}
    </figure>
    <TimelinePointDetails point={selected} origin={origin} sources={sources} reply={reply} onClose={onClose} />
  </>;
}
