import { useCallback, useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { RefreshCw, Wrench } from "lucide-react";
import { getToolDiagnostics, type ToolDiagnostic } from "../../api/system";
import { Badge, Button, CodeBlock, Drawer, EmptyState, ErrorState, Input, KeyValueRow, Select, Skeleton } from "../../components/ui";
import { formatLogTimestamp } from "../../components/system";
import { conversationLogPath, filterToolDiagnostics, toolDiagnosticStatus } from "./toolDiagnostics";
import "./toolDiagnostics.css";

export default function ToolDiagnosticsPanel() {
  const [status, setStatus] = useState<"error" | "all">("error");
  const [entries, setEntries] = useState<ToolDiagnostic[]>([]);
  const [total, setTotal] = useState(0);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [selected, setSelected] = useState<ToolDiagnostic | null>(null);
  const refresh = useCallback(async () => {
    const response = await getToolDiagnostics({ status, count: 500 });
    setLoading(false);
    if (!response) { setError("工具执行记录暂时无法加载"); return; }
    setError(""); setEntries(response.entries); setTotal(response.total);
    setSelected(current => current ? response.entries.find(entry => entry.conversationId === current.conversationId && entry.toolCallId === current.toolCallId) ?? current : null);
  }, [status]);
  useEffect(() => {
    setLoading(true);
    void refresh();
    const timer = window.setInterval(() => void refresh(), 3000);
    return () => window.clearInterval(timer);
  }, [refresh]);
  const visible = useMemo(() => filterToolDiagnostics(entries, search), [entries, search]);
  return <section className="tool-diagnostics" aria-label="工具执行日志">
    <div className="tool-diagnostics-toolbar">
      <Input aria-label="搜索工具执行记录" placeholder="工具、会话、参数或错误…" value={search} onChange={event => setSearch(event.target.value)} />
      <Select aria-label="工具执行状态" value={status} size="sm" options={[{ value: "error", label: "仅看报错" }, { value: "all", label: "全部执行" }]} onChange={event => { setEntries([]); setStatus(event.target.value as "error" | "all"); }} />
      <Button variant="secondary" size="sm" onClick={() => void refresh()}><RefreshCw className="h-3.5 w-3.5" />刷新</Button>
    </div>
    <p className="tool-diagnostics-hint">记录保存在对应会话中，可查看工具参数、返回结果和验证原因。系统日志清空不会删除这些记录。</p>
    {error ? <ErrorState title={error} action={<Button variant="secondary" onClick={() => void refresh()}>重试</Button>} /> : null}
    {loading ? <Skeleton className="h-24" /> : !error && visible.length === 0 ? <EmptyState icon={<Wrench className="h-7 w-7" />} title={status === "error" ? "暂无工具报错" : "暂无工具执行记录"} description={search ? "调整搜索词后重试。" : "执行工具后，记录会显示在这里。"} /> : null}
    {visible.length > 0 ? <div className="tool-diagnostics-table-scroll"><table className="tool-diagnostics-table"><thead><tr><th>时间</th><th>工具</th><th>状态</th><th>会话</th><th>结果 / 原因</th></tr></thead><tbody>{visible.map(entry => {
      const state = toolDiagnosticStatus(entry);
      return <tr key={`${entry.conversationId}:${entry.toolCallId}`}><td><time>{formatLogTimestamp(entry.updatedAt)}</time></td><td><button type="button" onClick={() => setSelected(entry)} aria-label={`查看工具执行 ${entry.name} ${entry.toolCallId}`}>{entry.name}</button></td><td><Badge tone={state.tone}>{state.label}</Badge></td><td title={entry.conversationTitle}>{entry.conversationTitle}</td><td><button type="button" className="tool-diagnostic-summary" onClick={() => setSelected(entry)}>{entry.verificationStatus === "failed" ? entry.verificationReason : entry.result ?? entry.reason ?? "暂无返回内容"}</button></td></tr>;
    })}</tbody></table></div> : null}
    <div className="tool-diagnostics-footer">显示 {visible.length} 条 / 符合状态 {total} 条 · 最近 500 条 · 自动刷新</div>
    <Drawer open={selected !== null} side="right" title="工具执行详情" width={440} onClose={() => setSelected(null)}>
      {selected ? <div className="tool-diagnostic-detail">
        <Link className="tool-diagnostic-conversation" to={conversationLogPath(selected)}>返回对应会话 →</Link>
        <div className="tool-diagnostic-metadata"><KeyValueRow label="工具" value={selected.name} /><KeyValueRow label="执行状态" value={toolDiagnosticStatus(selected).label} /><KeyValueRow label="会话" value={selected.conversationTitle} /><KeyValueRow label="调用 ID" value={selected.toolCallId} mono /><KeyValueRow label="更新于" value={formatLogTimestamp(selected.updatedAt)} /><KeyValueRow label="风险等级" value={selected.riskLevel} /><KeyValueRow label="验证状态" value={selected.verificationStatus} /></div>
        {selected.reason ? <section><h3>执行原因</h3><p>{selected.reason}</p></section> : null}
        <section><h3>调用参数</h3><CodeBlock code={JSON.stringify(selected.args, null, 2)} maxHeight="280px" /></section>
        <section><h3>返回结果</h3><CodeBlock code={selected.result ?? "尚无返回结果"} maxHeight="360px" /></section>
        {selected.verificationReason ? <section><h3>验证原因</h3><p>{selected.verificationReason}</p></section> : null}
      </div> : null}
    </Drawer>
  </section>;
}
