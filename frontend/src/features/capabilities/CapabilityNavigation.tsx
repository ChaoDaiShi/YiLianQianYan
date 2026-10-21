import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Brain, GitBranch, Link, Wrench, Puzzle, FolderInput } from "lucide-react";
import { Modal } from "../../components/ui";
import SkillsPage from "../../pages/SkillsPage";
import PluginsPage from "../../pages/PluginsPage";
import WorkflowsPage from "../../pages/WorkflowsPage";
import ToolManager from "./ToolManager";
import ManagedImports from "./ManagedImports";
import LocalServiceDiscovery from "./LocalServiceDiscovery";
import StructuredImport from "./StructuredImport";
import "./capabilityNavigation.css";
export const CAPABILITY_CATEGORIES = [
  { id: "skill", label: "Skill", description: "可复用的知识与操作方法", icon: Brain },
  { id: "workflow", label: "工作流", description: "编排、运行与检查任务流程", icon: GitBranch },
  { id: "mcp", label: "MCP", description: "连接外部工具、资源与提示词", icon: Link },
  { id: "tool", label: "工具", description: "查看真实工具与运行状态", icon: Wrench },
  { id: "plugin", label: "插件", description: "管理已导入的声明式扩展", icon: Puzzle },
  { id: "import", label: "导入与本地发现", description: "添加文件、目录与本地服务", icon: FolderInput },
] as const;
export type CapabilityCategory = typeof CAPABILITY_CATEGORIES[number]["id"];
export function CapabilityManagerDialog({ category, onClose }: { category: CapabilityCategory | null; onClose: () => void }) {
  const [params] = useSearchParams();
  const label = CAPABILITY_CATEGORIES.find(item => item.id === category)?.label;
  return <Modal open={category !== null} onClose={onClose} title={`${label ?? "能力"}管理`} size="xl" className="capability-manager-dialog">
    <div className="capability-manager-content" key={category}>
      {category === "skill" && <SkillsPage embedded initialSkill={params.get("skill") ?? undefined} />}
      {category === "workflow" && <WorkflowsPage embedded initialGraphId={params.get("workflow") ?? undefined} />}
      {category === "mcp" && <PluginsPage embedded initialServerId={params.get("server") ?? undefined} />}
      {category === "tool" && <ToolManager />}
      {category === "plugin" && <ManagedImports kind="plugin" />}
      {category === "import" && <><ManagedImports /><StructuredImport /><LocalServiceDiscovery /></>}
    </div>
  </Modal>;
}
export default function CapabilityNavigation() {
  const [params, setParams] = useSearchParams();
  const requested = params.get("category");
  const [category, setCategory] = useState<CapabilityCategory | null>(() => CAPABILITY_CATEGORIES.some(item => item.id === requested) ? requested as CapabilityCategory : null);
  useEffect(() => { if (CAPABILITY_CATEGORIES.some(item => item.id === requested)) setCategory(requested as CapabilityCategory); }, [requested]);
  return <div className="capability-navigation page-canvas"><div className="capability-navigation-inner"><header><h1>能力</h1><p>为忆涟选择方法、工具与扩展</p></header><div className="capability-navigation-grid">{CAPABILITY_CATEGORIES.map(({ id, label, description, icon: Icon }) => <button type="button" className="capability-navigation-card" key={id} onClick={() => setCategory(id)}><span className="capability-navigation-icon"><Icon size={27} /></span><strong>{label}</strong><span>{description}</span></button>)}</div></div><CapabilityManagerDialog category={category} onClose={() => { setCategory(null); setParams({}); }} /></div>;
}
