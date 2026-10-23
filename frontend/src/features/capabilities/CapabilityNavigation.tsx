import { useSearchParams } from "react-router-dom";
import { Brain, GitBranch, Link, Wrench, Puzzle, FolderInput } from "lucide-react";
import PageShell from "../../components/layout/PageShell";
import SkillsPage from "../../pages/SkillsPage";
import PluginsPage from "../../pages/PluginsPage";
import WorkflowsPage from "../../pages/WorkflowsPage";
import ToolManager from "./ToolManager";
import ManagedImports from "./ManagedImports";
import CapabilityImportWorkspace from "./CapabilityImportWorkspace";
import "./capabilityNavigation.css";

export const CAPABILITY_CATEGORIES = [
  { id: "skill", label: "Skill", description: "搜索已发现的技能，查看来源与完整使用说明", icon: Brain },
  { id: "workflow", label: "工作流", description: "编排执行步骤，查看运行结果与审批", icon: GitBranch },
  { id: "mcp", label: "MCP", description: "管理服务连接、工具、资源与提示词", icon: Link },
  { id: "tool", label: "工具", description: "查看当前运行时工具及调用要求", icon: Wrench },
  { id: "plugin", label: "插件", description: "管理已安装插件、可用状态与版本", icon: Puzzle },
  { id: "import", label: "导入与本地发现", description: "导入已有能力，发现本机服务与资料", icon: FolderInput },
] as const;
export type CapabilityCategory = typeof CAPABILITY_CATEGORIES[number]["id"];

export function CapabilityCategoryContent({ category }: { category: CapabilityCategory }) {
  const [params] = useSearchParams();
  switch (category) {
    case "skill": return <SkillsPage embedded initialSkill={params.get("skill") ?? undefined} />;
    case "workflow": return <WorkflowsPage embedded initialGraphId={params.get("workflow") ?? undefined} />;
    case "mcp": return <PluginsPage embedded initialServerId={params.get("server") ?? undefined} />;
    case "tool": return <ToolManager initialTool={params.get("tool") ?? undefined} />;
    case "plugin": return <ManagedImports kind="plugin" initialId={params.get("plugin") ?? undefined} />;
    case "import": return <CapabilityImportWorkspace />;
  }
}

export default function CapabilityNavigation() {
  const [params, setParams] = useSearchParams();
  const selected = CAPABILITY_CATEGORIES.find(item => item.id === params.get("category")) ?? CAPABILITY_CATEGORIES[0];
  return <PageShell title="能力" description="为忆涟选择方法、工具与扩展" variant="workspace" className="capability-workbench" contentClassName="capability-workbench-body" headerClassName="capability-workbench-header">
    <nav className="capability-category-nav" aria-label="能力分类">{CAPABILITY_CATEGORIES.map(({ id, label, icon: Icon }) => <button key={id} type="button" aria-current={selected.id === id ? "page" : undefined} onClick={() => setParams(current => { const next = new URLSearchParams(current); next.set("category", id); return next; })}><Icon size={16} /><span>{label}</span></button>)}</nav>
    <section className="capability-workbench-main" aria-label="能力工作台">
      <header className="capability-category-heading"><h2>{selected.label}</h2><p>{selected.description}</p></header>
      <div className="capability-workbench-content" key={selected.id}><CapabilityCategoryContent category={selected.id} /></div>
    </section>
  </PageShell>;
}
