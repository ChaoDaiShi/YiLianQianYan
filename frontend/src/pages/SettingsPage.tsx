import { lazy, Suspense } from "react";
import { Link } from "react-router-dom";
import PageShell from "../components/layout/PageShell";
import ResourcePolicyDialog from "../features/settings/sections/ResourcePolicyDialog";
import PluginSettingsDialog from "../features/settings/canvas/PluginSettingsDialog";
import SettingsEditorDialog from "../features/settings/editor/SettingsEditorDialog";
import WorkspaceServiceOverridesDialog from "../features/settings/editor/WorkspaceServiceOverridesDialog";
import { settingLabels, useSettingsSection, workspaceServicesSection } from "../features/settings/model/useSettingsSection";

const SettingsCanvasPage = lazy(() => import("../features/settings/canvas/SettingsCanvasPage"));

const managementLinks = [
  { label: "个人资料与初始化", path: "/setup?profile=1" },
  { label: "账号", path: "/account" },
  { label: "同步", path: "/account?section=sync" },
  { label: "数据与隐私", path: "/account?section=privacy" },
  { label: "记忆", path: "/memory" },
  { label: "浏览器", path: "/browser" },
  { label: "定时任务", path: "/automations" },
  { label: "任务画布", path: "/tasks" },
  { label: "工作空间", path: "/workspaces" },
];

interface SettingsGroupProps {
  title: string;
  sections: string[];
  onOpen: (id: string) => void;
  advanced?: boolean;
}

function SettingsGroup({ title, sections, onOpen, advanced }: SettingsGroupProps) {
  const Heading = advanced ? "h3" : "h2";
  return (
    <section className="settings-overview-group">
      <Heading>{title}</Heading>
      <div className="settings-overview-list">
        {sections.map((id) => (
          <button key={id} type="button" className="settings-overview-entry" onClick={() => onOpen(id)}>
            {settingLabels.get(id)}
          </button>
        ))}
      </div>
    </section>
  );
}

export default function SettingsPage() {
  const { section, setSection, isCanvasView } = useSettingsSection();

  if (isCanvasView) {
    return (
      <Suspense fallback={<PageShell title="设置"><p role="status">正在打开设置画布…</p></PageShell>}>
        <SettingsCanvasPage />
      </Suspense>
    );
  }

  return (
    <PageShell title="全局设置" maxWidth="dashboard" className="settings-overview" actions={
      <Link className="focus-ring-token rounded-md px-3 py-2 text-sm" to="/settings?view=canvas">查看设置画布</Link>
    }>
      <div className="settings-overview-grid">
        <SettingsGroup title="模型与语音" sections={["model", "stt", "tts", workspaceServicesSection]} onOpen={setSection} />
        <SettingsGroup title="界面与权限" sections={["appearance", "navigation", "permissions"]} onOpen={setSection} />
      </div>
      <nav className="settings-overview-group mt-4" aria-labelledby="settings-management-heading">
        <h2 id="settings-management-heading">平台管理</h2>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <button type="button" className="focus-ring-token rounded-md border border-[var(--border-soft)] px-3 py-2 text-left text-sm" onClick={() => setSection("resource-policy")}>运行资源</button>
          {managementLinks.map(({ label, path }) => (
            <Link key={path} to={path} className="focus-ring-token min-w-0 rounded-md border border-[var(--border-soft)] px-3 py-2 text-sm hover:bg-[var(--surface-hover)]">
              {label}
            </Link>
          ))}
        </div>
      </nav>
      <details className="settings-overview-more">
        <summary>更多设置</summary>
        <div className="settings-overview-grid">
          <SettingsGroup title="其他服务" sections={["search", "vision", "image", "embedding"]} onOpen={setSection} advanced />
          <SettingsGroup title="高级配置" sections={["model-pools", "agent", "compaction", "sandbox", "extensions"]} onOpen={setSection} advanced />
        </div>
      </details>
      {section === "resource-policy" ? <ResourcePolicyDialog onClose={() => setSection(null)} /> : section === workspaceServicesSection
        ? <WorkspaceServiceOverridesDialog onClose={() => setSection(null)} />
        : section?.startsWith("plugin:")
        ? <PluginSettingsDialog key={section} nodeId={section} onClose={() => setSection(null)} />
        : <SettingsEditorDialog nodeKey={section} onClose={() => setSection(null)} />}
    </PageShell>
  );
}
