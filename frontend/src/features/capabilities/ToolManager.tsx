import { useCallback, useEffect, useMemo, useState } from "react";
import { ChevronLeft, ChevronRight, RefreshCw, Wrench } from "lucide-react";
import { listTools, type RuntimeTool } from "../../api/system";
import { formatToolDisplayName } from "../../components/chat/toolDisplay";
import { Button, EmptyState, ErrorState, Input } from "../../components/ui";
import "./toolManager.css";

const riskLabels = { low: "低风险", medium: "中风险", high: "高风险", critical: "关键风险" };

export default function ToolManager({ initialTool }: { initialTool?: string }) {
  const [tools, setTools] = useState<RuntimeTool[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(initialTool ?? null);
  const reload = useCallback(async () => {
    setLoading(true);
    setError("");
    const result = await listTools();
    if (result.ok) setTools(result.data); else setError(result.error);
    setLoading(false);
  }, []);
  useEffect(() => { void reload(); }, [reload]);
  const filtered = useMemo(() => {
    const value = query.trim().toLowerCase();
    return tools.filter(tool => `${tool.name} ${formatToolDisplayName(tool.name)} ${tool.description}`.toLowerCase().includes(value));
  }, [tools, query]);
  const tool = tools.find(item => item.name === selected);
  return <section className="tool-manager" aria-label="运行时工具">
    {tool ? <>
      <div className="tool-manager-toolbar"><Button variant="ghost" onClick={() => setSelected(null)}><ChevronLeft size={15}/>返回工具列表</Button></div>
      <div className="tool-manager-detail" aria-label="工具详情">
        <h4>{formatToolDisplayName(tool.name)}</h4><code>{tool.name}</code>
        <span className="tool-manager-risk" data-risk={tool.risk_level}>{riskLabels[tool.risk_level]}</span>
        <p>{tool.description}</p>
        <p className="tool-manager-help">工具调用遵循当前审批模式与工作空间权限。</p>
        <details><summary>查看调用参数</summary><pre>{JSON.stringify(tool.parameters, null, 2)}</pre></details>
      </div>
    </> : <>
      <div className="tool-manager-toolbar"><Input aria-label="搜索工具" placeholder="搜索工具…" value={query} onChange={event => setQuery(event.target.value)}/><Button variant="ghost" onClick={() => void reload()} disabled={loading} aria-label="刷新工具"><RefreshCw size={15}/></Button></div>
      <p className="tool-manager-count">当前运行时已注册 {tools.length} 个工具</p>
      {error ? <ErrorState title="工具读取失败" description={error} onRetry={() => void reload()}/> : loading ? <p role="status">正在读取工具…</p> : <div className="tool-manager-list" aria-label="工具列表">
        {filtered.length ? filtered.map(item => <button type="button" key={item.name} className="tool-manager-row" onClick={() => setSelected(item.name)}>
          <Wrench size={17}/><span><strong>{formatToolDisplayName(item.name)}</strong><small>{item.description}</small></span><span className="tool-manager-risk" data-risk={item.risk_level}>{riskLabels[item.risk_level]}</span><ChevronRight size={15}/>
        </button>) : <EmptyState title="没有匹配的工具"/>}
      </div>}
    </>}
  </section>;
}
