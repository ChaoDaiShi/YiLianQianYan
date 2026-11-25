import { useCallback, useEffect, useMemo, useState } from "react";
import { RefreshCw, Wrench } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";
import { listTools, type RuntimeTool } from "../../api/system";
import { formatToolDisplayName } from "../../components/chat/toolDisplay";
import { Badge, Button, EmptyState, ErrorState, Input, Select } from "../../components/ui";
import "./toolManager.css";
import "./capabilityNavigation.css";

const riskLabels = { low: "低风险", medium: "中风险", high: "高风险", critical: "关键风险" };
const TOOL_GROUPS = [
  { id: "desktop", label: "操作电脑", matches: (name: string) => ["screenshot", "mouse", "keyboard", "process", "bash", "open_application"].includes(name) || name.startsWith("software_") },
  { id: "browser", label: "浏览网页", matches: (name: string) => name === "open_url" || name.startsWith("browser_") || name.startsWith("visible_browser_") },
  { id: "files", label: "管理文件", matches: (name: string) => ["read_file", "write_file", "edit_file", "glob", "grep"].includes(name) },
  { id: "memory", label: "管理记忆", matches: (name: string) => name.endsWith("_memory") || name.endsWith("_memories") },
];

export default function ToolManager({ initialTool }: { initialTool?: string }) {
  const [, setParams] = useSearchParams();
  const [tools, setTools] = useState<RuntimeTool[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [risk, setRisk] = useState("all");
  const [group, setGroup] = useState("all");
  const [selected, setSelected] = useState(initialTool ?? null);
  const reload = useCallback(async () => {
    setLoading(true); setError("");
    const result = await listTools();
    if (result.ok) setTools(result.data); else setError(result.error);
    setLoading(false);
  }, []);
  useEffect(() => { void reload(); }, [reload]);
  useEffect(() => { if (initialTool && initialTool !== selected) { setSelected(initialTool); setQuery(""); setRisk("all"); setGroup("all"); } }, [initialTool]);
  const filtered = useMemo(() => {
    const value = query.trim().toLowerCase();
    const selectedGroup = TOOL_GROUPS.find(item => item.id === group);
    return tools.filter(tool => (!selectedGroup || selectedGroup.matches(tool.name)) && (risk === "all" || tool.risk_level === risk) && `${tool.name} ${formatToolDisplayName(tool.name)} ${tool.description}`.toLowerCase().includes(value));
  }, [tools, query, risk, group]);
  const tool = tools.find(item => item.name === selected);
  return <section className="tool-manager" aria-label="运行时工具">
    <div className="flex flex-wrap gap-2" role="group" aria-label="默认工具组">
      {[{ id: "all", label: "全部工具" }, ...TOOL_GROUPS].map(item => <Button key={item.id} size="sm"
        variant={group === item.id ? "secondary" : "ghost"} aria-pressed={group === item.id}
        onClick={() => { setGroup(item.id); setSelected(null); }}>{item.label}</Button>)}
    </div>
    <div className="capability-workbench-toolbar"><Input aria-label="搜索工具" placeholder="搜索工具名称或用途…" value={query} onChange={event => setQuery(event.target.value)} /><Select aria-label="工具风险筛选" value={risk} onChange={event => setRisk(event.target.value)} options={[{value:"all",label:"全部风险"},...Object.entries(riskLabels).map(([value,label])=>({value,label}))]} /><Button variant="secondary" onClick={() => void reload()} disabled={loading} aria-label="刷新工具"><RefreshCw size={15} />刷新</Button></div>
    {error && <ErrorState title="工具读取失败" description={error} onRetry={() => void reload()} />}
    <div className="capability-resource-split">
      <div className="capability-resource-list" aria-label="工具列表"><p className="capability-list-caption">{filtered.length} / {tools.length} 个已注册工具</p>{loading ? <p role="status">正在读取工具…</p> : filtered.length ? filtered.map(item => <button type="button" key={item.name} className="capability-resource-row tool-manager-row" aria-current={selected === item.name ? "true" : undefined} onClick={() => { setSelected(item.name); setParams(current => { const next = new URLSearchParams(current); next.set("tool", item.name); return next; }, { replace:true }); }}><span className="capability-resource-name"><Wrench size={15} /><strong>{formatToolDisplayName(item.name)}</strong></span><span>{item.description}</span><small>{riskLabels[item.risk_level]}</small></button>) : <EmptyState title="没有匹配的工具" />}</div>
      <div className="capability-resource-detail tool-manager-detail" aria-label="工具详情">{tool ? <>
        <h3>{formatToolDisplayName(tool.name)}</h3><p>{tool.description}</p><div><Badge tone={tool.risk_level === "high" || tool.risk_level === "critical" ? "warning" : "default"}>{riskLabels[tool.risk_level]}</Badge></div>
        <p>{tool.requires_approval ? "调用时需要按当前权限完成审批。" : "调用时使用当前工作空间权限。"}</p>
        <div className="flex flex-wrap gap-3 text-sm text-[var(--accent-primary)]"><Link to="/settings?section=permissions">权限设置</Link><Link to="/logs">执行记录</Link></div>
        <details><summary>技术详情与调用参数</summary><code>{tool.name}</code><pre>{JSON.stringify(tool.parameters, null, 2)}</pre></details>
      </> : <EmptyState title={selected && !loading ? "当前运行时未找到此工具" : "选择一个工具"} description="查看用途、权限要求与实际参数。" />}</div>
    </div>
  </section>;
}
