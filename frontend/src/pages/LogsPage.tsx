import { useState, useEffect, useRef, useCallback } from "react";
import {
  ScrollText, Pause, Play, Trash2, AlertTriangle,
  Info, AlertCircle, Bug, Wrench, MessageSquare,
} from "lucide-react";
import { getLogs, type LogEntry } from "../api/client";
import { PageHeader, Button } from "../components/ui";

const LEVEL_META: Record<string, { label: string; color: string; icon: React.ReactNode }> = {
  info: { label: "信息", color: "text-sky-400", icon: <Info className="w-3.5 h-3.5" /> },
  warn: { label: "警告", color: "text-amber-400", icon: <AlertCircle className="w-3.5 h-3.5" /> },
  error: { label: "错误", color: "text-red-400", icon: <AlertTriangle className="w-3.5 h-3.5" /> },
  debug: { label: "调试", color: "text-violet-400", icon: <Bug className="w-3.5 h-3.5" /> },
  tool: { label: "工具", color: "text-emerald-400", icon: <Wrench className="w-3.5 h-3.5" /> },
  chat: { label: "对话", color: "text-cyan-400", icon: <MessageSquare className="w-3.5 h-3.5" /> },
};

const SOURCE_LABELS: Record<string, string> = {
  api: "API",
  agent: "Agent",
  tool: "工具",
  system: "系统",
};

const LEVELS = ["", "info", "warn", "error", "debug", "tool", "chat"] as const;

export default function LogsPage() {
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [paused, setPaused] = useState(false);
  const [filterLevel, setFilterLevel] = useState("");
  const [filterSource, setFilterSource] = useState("");
  const [autoScroll, setAutoScroll] = useState(true);
  const listRef = useRef<HTMLDivElement>(null);
  const [total, setTotal] = useState(0);

  const fetchLogs = useCallback(async () => {
    if (paused) return;
    const res = await getLogs({
      count: 500,
      level: filterLevel || undefined,
      source: filterSource || undefined,
    });
    if (res) {
      // Only append new entries (by timestamp + message dedup)
      setEntries((prev) => {
        const existingKeys = new Set(prev.map((e) => `${e.timestamp}-${e.message}`));
        const fresh = res.entries.filter((e) => !existingKeys.has(`${e.timestamp}-${e.message}`));
        const merged = [...prev, ...fresh].slice(-500);
        return merged;
      });
      setTotal(res.total);
    }
  }, [paused, filterLevel, filterSource]);

  // Initial load
  useEffect(() => {
    fetchLogs();
  }, [fetchLogs]);

  // Auto-refresh every 2 seconds
  useEffect(() => {
    const t = setInterval(fetchLogs, 2000);
    return () => clearInterval(t);
  }, [fetchLogs]);

  // Auto-scroll
  useEffect(() => {
    if (autoScroll && listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [entries, autoScroll]);

  const clearLogs = async () => {
    await getLogs({ drain: true });
    setEntries([]);
    setTotal(0);
  };

  const formatTime = (ts: number) => {
    const d = new Date(ts);
    return d.toLocaleTimeString("zh-CN", { hour12: false }) + "." + String(d.getMilliseconds()).padStart(3, "0");
  };

  const meta = (level: string) => LEVEL_META[level] || LEVEL_META.info;

  const handleScroll = () => {
    if (!listRef.current) return;
    const el = listRef.current;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    setAutoScroll(atBottom);
  };

  return (
    <div className="flex flex-col h-full">
      <PageHeader
        title="运行日志"
        description={`共 ${total} 条 · ${paused ? "已暂停" : "自动刷新中"}`}
        actions={
          <div className="flex items-center gap-2">
            {/* Level filter */}
            <select
              value={filterLevel}
              onChange={(e) => {
                setFilterLevel(e.target.value);
                setEntries([]);
              }}
              className="px-2.5 py-1.5 rounded-lg border border-[var(--border)] bg-[var(--input-bg)] text-xs text-[var(--text)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]/40"
            >
              <option value="">全部级别</option>
              {LEVELS.filter(Boolean).map((l) => (
                <option key={l} value={l}>{LEVEL_META[l]?.label || l}</option>
              ))}
            </select>

            {/* Source filter */}
            <select
              value={filterSource}
              onChange={(e) => {
                setFilterSource(e.target.value);
                setEntries([]);
              }}
              className="px-2.5 py-1.5 rounded-lg border border-[var(--border)] bg-[var(--input-bg)] text-xs text-[var(--text)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]/40"
            >
              <option value="">全部来源</option>
              {Object.entries(SOURCE_LABELS).map(([k, v]) => (
                <option key={k} value={k}>{v}</option>
              ))}
            </select>

            <div className="w-px h-5 bg-[var(--border)]" />

            <Button
              variant="secondary"
              size="sm"
              onClick={() => setPaused(!paused)}
              title={paused ? "恢复自动刷新" : "暂停刷新"}
            >
              {paused ? <Play className="w-3.5 h-3.5" /> : <Pause className="w-3.5 h-3.5" />}
              {paused ? "继续" : "暂停"}
            </Button>
            <Button variant="secondary" size="sm" onClick={clearLogs} title="清空日志">
              <Trash2 className="w-3.5 h-3.5" />
              清空
            </Button>
          </div>
        }
      />

      {/* Log list — terminal style */}
      <div
        ref={listRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto scrollbar-thin bg-[var(--bg-2)] font-mono text-xs leading-5"
      >
        {entries.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-[var(--text-faint)] gap-3">
            <ScrollText className="w-10 h-10" />
            <p>暂无日志，等待事件…</p>
          </div>
        ) : (
          <div className="py-1">
            {entries.map((e, i) => {
              const m = meta(e.level);
              return (
                <div
                  key={`${e.timestamp}-${i}`}
                  className="flex items-start gap-2 px-3 py-0.5 hover:bg-[var(--panel-hover)]/50 transition-colors group"
                >
                  {/* Timestamp */}
                  <span className="text-[var(--text-faint)] flex-shrink-0 select-none">
                    {formatTime(e.timestamp)}
                  </span>

                  {/* Level icon */}
                  <span className={`${m.color} flex-shrink-0 mt-px`} title={m.label}>
                    {m.icon}
                  </span>

                  {/* Source badge */}
                  <span className="px-1 py-px rounded text-[10px] bg-[var(--panel-2)] text-[var(--text-muted)] flex-shrink-0 select-none">
                    {SOURCE_LABELS[e.source] || e.source}
                  </span>

                  {/* Message */}
                  <span className="text-[var(--text)] break-all">{e.message}</span>

                  {/* Copy on hover */}
                  <button
                    onClick={() => navigator.clipboard.writeText(e.message)}
                    className="ml-auto opacity-0 group-hover:opacity-100 text-[var(--text-faint)] hover:text-[var(--text)] flex-shrink-0 px-1 select-none"
                    title="复制"
                  >
                    ⧉
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Bottom bar */}
      <div className="flex items-center gap-3 px-4 py-1.5 border-t border-[var(--border)] bg-[var(--panel)]/60 text-xs text-[var(--text-faint)]">
        <span className="flex items-center gap-1.5">
          <span className={`w-2 h-2 rounded-full ${paused ? "bg-[var(--warning)]" : "bg-[var(--success)]"} ${!paused ? "animate-pulse" : ""}`} />
          {paused ? "已暂停" : "实时"}
        </span>
        <span className="text-[var(--border)]">|</span>
        <span>{entries.length} 条显示</span>
        <span className="text-[var(--border)]">|</span>
        <span>level={filterLevel || "all"} source={filterSource || "all"}</span>
        <span className="ml-auto">
          {autoScroll ? "自动滚动" : "滚动锁定"}
        </span>
      </div>
    </div>
  );
}
