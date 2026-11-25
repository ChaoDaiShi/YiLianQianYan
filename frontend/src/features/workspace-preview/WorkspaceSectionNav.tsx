import { Link, useNavigate } from "react-router-dom";
import { Tabs } from "../../components/ui";
import { pluginFeaturePath } from "../../api/plugins";
import { usePluginFeatures } from "../plugins/PluginFeaturePage";

export type WorkspaceSection = "files" | "pages" | "learning" | "creation" | "tasks" | "artifacts";
const sections = [
  { id: "files", label: "文件", panelId: false as const },
  { id: "pages", label: "页面", panelId: false as const },
  { id: "learning", label: "资料学习", panelId: false as const },
  { id: "creation", label: "创作", panelId: false as const },
  { id: "tasks", label: "任务", panelId: false as const },
  { id: "artifacts", label: "成果", panelId: false as const },
];

export default function WorkspaceSectionNav({ workspaceId, active, onChange }: { workspaceId: string; active: WorkspaceSection; onChange?: (section: WorkspaceSection) => void }) {
  const navigate = useNavigate();
  const { features } = usePluginFeatures();
  const extensions = features.filter(feature => feature.placements?.includes("workspace"));
  return <div className="workspace-detail-navigation">
    <Tabs tabs={sections} activeId={active} size="sm" className="workspace-section-tabs" aria-label="工作空间内容" onChange={id => {
      const section = id as WorkspaceSection;
      if (onChange) { onChange(section); return; }
      const root = `/workspaces/${encodeURIComponent(workspaceId)}`;
      navigate(section === "learning" || section === "creation" ? `${root}/${section}` : `${root}?tab=${section}`);
    }} />
    {extensions.length > 0 && <nav aria-label="工作空间插件">
      {extensions.map(feature => <Link key={`${feature.plugin_id}:${feature.id}`} title={feature.plugin_name}
        to={pluginFeaturePath(feature)}>{feature.title}</Link>)}
    </nav>}
  </div>;
}
