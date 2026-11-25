import { useSearchParams } from "react-router-dom";
import PageShell from "../../components/layout/PageShell";
import { CapabilityRegistryContent } from "./CapabilityRegistry";
import SkillsPage from "../../pages/SkillsPage";
import PluginsPage from "../../pages/PluginsPage";
import WorkflowsPage from "../../pages/WorkflowsPage";
import ToolManager from "./ToolManager";
import HarnessManager from "./HarnessManager";
import ManagedImports from "./ManagedImports";
import CapabilityImportWorkspace from "./CapabilityImportWorkspace";
import WorkspaceIcon, { type WorkspaceIconName } from "../../components/ui/WorkspaceIcon";
import "./capabilityNavigation.css";

export const CAPABILITY_CATEGORIES = [
  { id: "skill", label: "技能" },
  { id: "workflow", label: "工作流" },
  { id: "mcp", label: "MCP" },
  { id: "tool", label: "工具" },
  { id: "harness", label: "Harness" },
  { id: "plugin", label: "插件" },
  { id: "import", label: "导入与本地发现" },
  { id: "catalog", label: "全部能力" },
] as const;
export type CapabilityCategory = typeof CAPABILITY_CATEGORIES[number]["id"];

const CATEGORY_GROUPS: { label: string; ids: CapabilityCategory[] }[] = [
  { label: "能力组合", ids: ["harness"] },
  { label: "能力组件", ids: ["skill", "tool", "workflow"] },
  { label: "来源与总览", ids: ["mcp", "plugin", "import", "catalog"] },
];
const CATEGORY_LABELS = new Map(CAPABILITY_CATEGORIES.map(item => [item.id, item.label]));

const CATEGORY_WORKSPACE_ICONS: Record<CapabilityCategory, WorkspaceIconName> = {
  catalog: "tool-catalog",
  skill: "skill",
  mcp: "mcp",
  import: "import-discover",
  workflow: "task-flow",
  tool: "tool-catalog",
  harness: "task-flow",
  plugin: "extensions",
};

export function CapabilityCategoryContent({ category }: { category: CapabilityCategory }) {
  const [params] = useSearchParams();
  switch (category) {
    case "catalog": return <CapabilityRegistryContent />;
    case "skill": return <SkillsPage embedded initialSkill={params.get("skill") ?? undefined} />;
    case "workflow": return <WorkflowsPage key={params.get("workflow") ?? "workflows"} embedded initialGraphId={params.get("workflow") ?? undefined} />;
    case "mcp": return <PluginsPage embedded initialServerId={params.get("server") ?? undefined} />;
    case "tool": return <ToolManager initialTool={params.get("tool") ?? undefined} />;
    case "harness": return <HarnessManager />;
    case "plugin": return <ManagedImports kind="plugin" initialId={params.get("plugin") ?? undefined} />;
    case "import": return <CapabilityImportWorkspace />;
  }
}

export default function CapabilityNavigation() {
  const [params, setParams] = useSearchParams();
  const selected = CAPABILITY_CATEGORIES.find(item => item.id === params.get("category")) ?? CAPABILITY_CATEGORIES[0];
  function selectCategory(id: CapabilityCategory) {
    const next = new URLSearchParams(params);
    next.set("category", id);
    setParams(next);
  }

  return <PageShell title="能力" description="管理忆涟可用的技能、工具、工作流与扩展" variant="workspace" className="capability-workbench" contentClassName="capability-workbench-body" headerClassName="capability-workbench-header">
    <nav className="capability-category-nav" aria-label="能力分类">
      {CATEGORY_GROUPS.map(group => <div key={group.label} className="capability-category-group" role="group" aria-label={group.label}>
        <span className="capability-category-group-label" aria-hidden="true">{group.label}</span>
        {group.ids.map(id => <button key={id} type="button" aria-current={selected.id === id ? "page" : undefined} onClick={() => selectCategory(id)}>
          <span className="capability-category-icon"><WorkspaceIcon name={CATEGORY_WORKSPACE_ICONS[id]} size={24} variant="mono" /></span>
          <span>{CATEGORY_LABELS.get(id)}</span>
        </button>)}
      </div>)}
    </nav>
    <section className="capability-workbench-main" aria-label="能力工作台">
      <div className="capability-workbench-content" key={selected.id}><CapabilityCategoryContent category={selected.id} /></div>
    </section>
  </PageShell>;
}
