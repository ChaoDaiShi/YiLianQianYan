import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowDown, Check, Clipboard, Copy, Pause, Play, RefreshCw, ScrollText, Trash2 } from "lucide-react";
import { getLogs, type LogEntry } from "../api/client";
import { Drawer, EmptyState, ErrorState, Input, PageHeader, Skeleton, Button, Badge } from "../components/ui";
import { formatLogLevel, formatLogTimestamp } from "../components/system";

function highlightMatch(text: string, query: string) {
  if (!query) return text;
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const parts = text.split(new RegExp(`(${escaped})`, "gi"));
  if (parts.length <= 1) return text;
  return parts.map((part, index) =>
    part.toLowerCase() === query.toLowerCase() ? (
      <mark key={index} className="ui-opt-log-highlight">{part}</mark>
    ) : (
      part
    )
  );
}

function isSameEntry(a: LogEntry | null | undefined, b: LogEntry | null | undefined) {
  if (!a || !b) return false;
  return a.timestamp === b.timestamp && a.level === b.level && a.source === b.source && a.message === b.message;
}

function LogRow({
  entry,
  selected,
  searchQuery,
  onOpen,
}: {
  entry: LogEntry;
  selected: boolean;
  searchQuery: string;
  onOpen: () => void;
}) {
  const level = formatLogLevel(entry.level);
  const isWarn = entry.level?.toLowerCase() === "warn";
  const isError = entry.level?.toLowerCase() === "error";

  return (
    <div
      className={`system-log-row group ${selected ? "ui-opt-log-selected" : ""} ${isError ? "ui-opt-log-error" : isWarn ? "ui-opt-log-warn" : ""}`}
      role="button"
      tabIndex={0}
      aria-label={`查看 ${level.label} 日志：${entry.message}`}
      onClick={onOpen}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpen();
        }
      }}
    >
      <time className="system-log-time" dateTime={new Date(entry.timestamp).toISOString()}>{formatLogTimestamp(entry.timestamp)}</time>
      <Badge tone={level.tone}>{level.label}</Badge>
      <span className="system-log-source">{highlightMatch(entry.source || "—", searchQuery)}</span>
      <span className="system-log-message">{highlightMatch(entry.message, searchQuery)}</span>
      <button
        type="button"
        className="system-log-copy"
        aria-label="复制日志内容"
        title="复制日志内容"
        onClick={(event) => {
          event.stopPropagation();
          void navigator.clipboard?.writeText(entry.message);
        }}
      >
        <Clipboard className="h-3.5 w-3.5" aria-hidden="true" />
      </button>
    </div>
  );
}

export default function LogsPage() {
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [paused, setPaused] = useState(false);
  const [filterLevel, setFilterLevel] = useState("");
  const [filterSource, setFilterSource] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const [autoScroll, setAutoScroll] = useState(true);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  const [selectedEntry, setSelectedEntry] = useState<LogEntry | null>(null);
  const [copiedDetail, setCopiedDetail] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);

  const fetchLogs = useCallback(async () => {
    if (paused) return;
    const response = await getLogs({
      count: 500,
      level: filterLevel || undefined,
      source: filterSource || undefined,
    });
    if (!response) {
      setLoadError("日志暂时无法加载");
      setLoading(false);
      return;
    }

    setLoadError("");
    setEntries((previous) => {
      const existingKeys = new Set(previous.map((entry) => `${entry.timestamp}-${entry.level}-${entry.source}-${entry.message}`));
      const fresh = response.entries.filter((entry) => !existingKeys.has(`${entry.timestamp}-${entry.level}-${entry.source}-${entry.message}`));
      return [...previous, ...fresh].slice(-500);
    });
    setTotal(response.total);
    setLoading(false);
  }, [filterLevel, filterSource, paused]);

  useEffect(() => {
    void fetchLogs();
  }, [fetchLogs]);

  useEffect(() => {
    const timer = window.setInterval(() => void fetchLogs(), 2000);
    return () => window.clearInterval(timer);
  }, [fetchLogs]);

  useEffect(() => {
    if (autoScroll && listRef.current) listRef.current.scrollTop = listRef.current.scrollHeight;
  }, [entries, autoScroll]);

  const sourceOptions = useMemo(
    () => Array.from(new Set(entries.map((entry) => entry.source).filter(Boolean))).sort(),
    [entries],
  );
  const levelOptions = useMemo(
    () => Array.from(new Set(entries.map((entry) => entry.level).filter(Boolean))).sort(),
    [entries],
  );
  const visibleEntries = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return entries;
    return entries.filter((entry) => `${entry.message} ${entry.source}`.toLowerCase().includes(query));
  }, [entries, searchQuery]);

  const handleFilterChange = (setter: (value: string) => void, value: string) => {
    setter(value);
    setEntries([]);
    setTotal(0);
    setLoadError("");
  };

  const handleScroll = () => {
    if (!listRef.current) return;
    const element = listRef.current;
    setAutoScroll(element.scrollHeight - element.scrollTop - element.clientHeight < 40);
  };

  const scrollToLatest = () => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
      setAutoScroll(true);
    }
  };

  const handleListKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT") {
      return;
    }
    if (visibleEntries.length === 0) return;

    if (event.key === "ArrowDown") {
      event.preventDefault();
      const currentIndex = selectedEntry ? visibleEntries.findIndex((e) => isSameEntry(e, selectedEntry)) : -1;
      const nextIndex = Math.min(visibleEntries.length - 1, currentIndex + 1);
      setSelectedEntry(visibleEntries[nextIndex]);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      const currentIndex = selectedEntry ? visibleEntries.findIndex((e) => isSameEntry(e, selectedEntry)) : visibleEntries.length;
      const prevIndex = Math.max(0, currentIndex - 1);
      setSelectedEntry(visibleEntries[prevIndex]);
    }
  };

  const clearLogs = async () => {
    const response = await getLogs({ drain: true });
    if (!response) {
      setLoadError("日志暂时无法加载");
      return;
    }
    setEntries([]);
    setTotal(0);
    setLoadError("");
  };

  const copyFullLog = (entry: LogEntry) => {
    const fullText = `[${formatLogTimestamp(entry.timestamp)}] [${entry.level.toUpperCase()}] [${entry.source || "system"}] ${entry.message}`;
    void navigator.clipboard?.writeText(fullText);
    setCopiedDetail(true);
    window.setTimeout(() => setCopiedDetail(false), 2000);
  };

  return (
    <div className="ui-opt-logs system-center-page page-canvas flex h-full min-h-0 flex-col">
      <PageHeader
        title="日志"
        description={`查看系统运行记录和异常信息 · 当前已加载 ${visibleEntries.length} 条（服务端总计约 ${total} 条）`}
        actions={(
          <div className="system-logs-toolbar flex flex-wrap items-center gap-2">
            <Input
              aria-label="搜索日志"
              placeholder="搜索消息或来源…"
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              className="system-log-search"
            />
            <label className="system-log-filter">
              <span>级别</span>
              <select aria-label="按日志级别筛选" value={filterLevel} onChange={(event) => handleFilterChange(setFilterLevel, event.target.value)}>
                <option value="">全部</option>
                {levelOptions.map((level) => <option key={level} value={level}>{formatLogLevel(level).label}</option>)}
              </select>
            </label>
            <label className="system-log-filter">
              <span>来源</span>
              <select aria-label="按日志来源筛选" value={filterSource} onChange={(event) => handleFilterChange(setFilterSource, event.target.value)}>
                <option value="">全部</option>
                {sourceOptions.map((source) => <option key={source} value={source}>{source}</option>)}
              </select>
            </label>
            <Button variant="secondary" size="sm" onClick={() => void fetchLogs()} aria-label="刷新日志">
              <RefreshCw className="h-3.5 w-3.5" />刷新
            </Button>
            <Button variant="secondary" size="sm" onClick={() => setPaused((value) => !value)} aria-pressed={paused}>
              {paused ? <Play className="h-3.5 w-3.5" /> : <Pause className="h-3.5 w-3.5" />}
              {paused ? "继续" : "暂停"}
            </Button>
            <Button variant="secondary" size="sm" onClick={() => void clearLogs()} aria-label="清空日志">
              <Trash2 className="h-3.5 w-3.5" />清空
            </Button>
          </div>
        )}
      />

      <div className="system-logs-surface relative min-h-0 flex-1 overflow-hidden px-4 pb-4 pt-4">
        {loadError ? (
          <ErrorState
            title="日志暂时无法加载"
            description="请稍后重新尝试。"
            action={<Button variant="secondary" onClick={() => void fetchLogs()}>重试</Button>}
          />
        ) : null}
        {loading && entries.length === 0 ? (
          <div className="system-log-list" aria-label="正在加载日志">
            {Array.from({ length: 6 }, (_, index) => <Skeleton key={index} className="h-10 w-full" />)}
          </div>
        ) : null}
        {!loading && !loadError && visibleEntries.length === 0 ? (
          <EmptyState icon={<ScrollText className="h-8 w-8" />} title="暂时没有日志记录。" description="新的系统事件会显示在这里。" />
        ) : null}
        {visibleEntries.length > 0 ? (
          <div
            ref={listRef}
            onScroll={handleScroll}
            onKeyDown={handleListKeyDown}
            tabIndex={0}
            className="system-log-list min-h-0 overflow-y-auto scrollbar-thin outline-none"
            aria-label="日志列表"
          >
            {visibleEntries.map((entry, index) => (
              <LogRow
                key={`${entry.timestamp}-${entry.source}-${index}`}
                entry={entry}
                selected={isSameEntry(entry, selectedEntry)}
                searchQuery={searchQuery}
                onOpen={() => setSelectedEntry(entry)}
              />
            ))}
          </div>
        ) : null}

        {!autoScroll && visibleEntries.length > 0 ? (
          <button
            type="button"
            className="ui-opt-scroll-bottom-btn"
            onClick={scrollToLatest}
            aria-label="回到最新日志"
          >
            <ArrowDown className="h-3.5 w-3.5" />
            回到最新
          </button>
        ) : null}

        <div className="system-logs-footer">
          <span>{visibleEntries.length} 条显示</span>
          <span>{paused ? "已暂停" : "自动刷新中"}</span>
          <span>{autoScroll ? "自动滚动" : "滚动锁定"}</span>
        </div>
      </div>

      <Drawer open={selectedEntry !== null} side="right" title="日志详情" onClose={() => setSelectedEntry(null)}>
        {selectedEntry ? (
          <div className="system-log-detail">
            <dl>
              <div><dt>时间</dt><dd>{formatLogTimestamp(selectedEntry.timestamp)}</dd></div>
              <div><dt>级别</dt><dd>{formatLogLevel(selectedEntry.level).label}</dd></div>
              <div><dt>来源</dt><dd>{selectedEntry.source || "—"}</dd></div>
            </dl>
            <section aria-labelledby="log-message-title">
              <div className="mb-2 flex items-center justify-between">
                <h3 id="log-message-title">消息</h3>
                <div className="flex gap-2">
                  <Button variant="secondary" size="sm" onClick={() => copyFullLog(selectedEntry)}>
                    {copiedDetail ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
                    {copiedDetail ? "已复制" : "复制完整日志"}
                  </Button>
                </div>
              </div>
              <pre className="overflow-x-auto whitespace-pre-wrap break-all rounded-lg border border-[var(--border-soft)] bg-[var(--surface-muted)] p-3 text-xs font-mono">{selectedEntry.message}</pre>
            </section>
          </div>
        ) : null}
      </Drawer>
    </div>
  );
}
