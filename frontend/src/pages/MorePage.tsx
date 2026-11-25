import { useEffect, useState } from "react";
import { ArrowRight } from "lucide-react";
import { Link, useNavigate } from "react-router-dom";
import PageShell from "../components/layout/PageShell";
import { NAV_ENTRIES } from "../components/layout/navRegistry";
import { useProductPreferences } from "../components/system/useProductPreferences";
import { usePluginFeatures } from "../features/plugins/PluginFeaturePage";
import { listWorkspaces, type Workspace } from "../api/workspaces";
import { pluginFeaturePath } from "../api/plugins";
import { closeFloatingSurface, desktopFloatingAvailable, isDesktopPetOpen, listenDesktopPetOpen, openDesktopPet } from "../features/voice/floatingSurface";
import { Button, EmptyState, ErrorState, Modal } from "../components/ui";
import WorkspaceIcon, { type WorkspaceIconName } from "../components/ui/WorkspaceIcon";
import DesktopSettingsDialog from "../surfaces/desktop/DesktopSettingsDialog";

const CONNECTIONS = [
  { to: "/browser", label: "浏览器", icon: "workspace-switch" },
  { to: "/software", label: "软件挂载", icon: "extensions" },
  { to: "/automations", label: "定时任务", icon: "task-calendar" },
] as const;

const PROJECT_TOOLS = [
  { id: "learning", label: "学习", icon: "learning" },
  { id: "creation", label: "创作", icon: "creation" },
] as const;

type Destination = (typeof PROJECT_TOOLS)[number]["id"];

function EntryIcon({ name }: { name: WorkspaceIconName }) {
  return <span className="navigation-entry-icon"><WorkspaceIcon name={name} size={24} /></span>;
}

function DesktopPetSwitch() {
  const [open, setOpen] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let active = true;
    let changed = false;
    let unlisten: (() => void) | undefined;
    async function readState() {
      const stop = await listenDesktopPetOpen(value => {
        if (!active) return;
        changed = true;
        setOpen(value);
        setError("");
      });
      if (!active) {
        stop();
        return;
      }
      unlisten = stop;
      const value = await isDesktopPetOpen();
      if (active && !changed) setOpen(value);
    }
    void readState().catch(() => {
      if (active && !changed) setError("无法读取桌宠状态。");
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [attempt]);

  async function toggle() {
    if (open === null || busy) return;
    setError("");
    setBusy(true);
    try {
      if (open) await closeFloatingSurface("pet");
      else await openDesktopPet();
    } catch {
      setError(open ? "桌宠未能关闭，请重试。" : "桌宠未能打开，请重试。");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <button type="button" className="navigation-entry more-pet-switch" role="switch"
        aria-label="桌宠" aria-checked={open === true} aria-disabled={open === null || busy}
        onClick={() => void toggle()}>
        <span>桌宠</span>
        <span className="more-pet-status" aria-hidden="true">
          {open === null ? (error ? "状态不可用" : "读取中") : open ? "已开启" : "已关闭"}
        </span>
        <span className="more-pet-track" aria-hidden="true" />
      </button>
      {error && <div className="more-pet-error">
        <span role="alert">{error}</span>
        {open === null && <Button size="sm" variant="ghost" onClick={() => {
          setError("");
          setAttempt(previous => previous + 1);
        }}>重试</Button>}
      </div>}
    </div>
  );
}

export default function MorePage() {
  const navigate = useNavigate();
  const { preferences } = useProductPreferences();
  const { features: pluginFeatures, error, reload, loading: pluginsLoading } = usePluginFeatures();
  const features = pluginFeatures.filter(feature => !feature.placements || feature.placements.includes("more"));
  const [destination, setDestination] = useState<Destination | null>(null);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspaceError, setWorkspaceError] = useState("");
  const [loading, setLoading] = useState(false);
  const [desktopSettings, setDesktopSettings] = useState(false);

  useEffect(() => {
    if (!destination) return;
    let active = true;
    void listWorkspaces().then(result => {
      if (!active) return;
      if (result.ok) setWorkspaces(result.data.filter(item => item.status === "active"));
      else setWorkspaceError(result.error);
      setLoading(false);
    });
    return () => { active = false; };
  }, [destination]);

  const visibleEntries = NAV_ENTRIES.filter(entry =>
    preferences.navigation.some(item => item.id === entry.id && item.visible)
  );
  const projectEntries = visibleEntries.filter(entry => entry.id === "memory" || entry.id === "knowledge");
  const adminEntries = visibleEntries.filter(entry => ["agents", "system", "logs"].includes(entry.id));

  return (
    <PageShell title="更多" maxWidth="none" className="preview-more-page">
      <div className="navigation-page preview-more">
        <nav aria-label="更多功能导航" className="more-directory">
          <section aria-labelledby="more-project-tools">
            <h2 id="more-project-tools">资料与创作</h2>
            <div className="navigation-card-grid">
              {projectEntries.map(entry => (
                <Link key={entry.id} to={entry.to} className="navigation-entry">
                  <EntryIcon name={entry.workspaceIcon} />{entry.label}
                </Link>
              ))}
              {PROJECT_TOOLS.map(tool => (
                <button key={tool.id} type="button" className="navigation-entry" onClick={() => {
                  setLoading(true);
                  setWorkspaceError("");
                  setDestination(tool.id);
                }}>
                  <EntryIcon name={tool.icon} />{tool.label}
                </button>
              ))}
            </div>
          </section>
          <section aria-labelledby="more-connections">
            <h2 id="more-connections">接入与自动化</h2>
            <div className="navigation-card-grid">
              {CONNECTIONS.map(entry => (
                <Link key={entry.to} to={entry.to} className="navigation-entry">
                  <EntryIcon name={entry.icon} />{entry.label}
                </Link>
              ))}
              <Link to="/capabilities?category=plugin" className="navigation-entry"><EntryIcon name="extensions" />管理插件</Link>
              {desktopFloatingAvailable() && <>
                <DesktopPetSwitch />
                <button type="button" className="navigation-entry" onClick={() => setDesktopSettings(true)}>
                  <EntryIcon name="extensions" />桌面与桌宠
                </button>
              </>}
            </div>
          </section>
          <section aria-labelledby="more-admin">
            <h2 id="more-admin">管理与诊断</h2>
            <div className="navigation-card-grid">
              {adminEntries.map(entry => (
                <Link key={entry.id} to={entry.to} className="navigation-entry">
                  <EntryIcon name={entry.workspaceIcon} />{entry.label}
                </Link>
              ))}
              <Link to="/more/manual" className="navigation-entry"><EntryIcon name="knowledge-base" />用户手册</Link>
            </div>
          </section>
        </nav>
        {(pluginsLoading || error || features.length > 0) && <section className="preview-plugin-section" aria-label="插件功能">
          {pluginsLoading ? <p role="status">正在读取插件功能…</p> : error ? (
            <ErrorState title="插件功能入口无法加载" description={error} action={<Button onClick={() => void reload()}>重试</Button>} />
          ) : (
            <>
              <h2>插件功能</h2>
              {features.length > 0 && (
                <div className="navigation-card-grid">
                  {features.map(feature => (
                    <Link key={feature.plugin_id + ":" + feature.id} className="navigation-entry"
                      to={pluginFeaturePath(feature)}>
                      <EntryIcon name="extensions" />
                      <span className="preview-plugin-name">{feature.title}<small>{feature.plugin_name}</small></span>
                    </Link>
                  ))}
                </div>
              )}
            </>
          )}
        </section>}
      </div>
      <Modal open={Boolean(destination)} onClose={() => setDestination(null)}
        title={destination === "learning" ? "选择学习工作空间" : "选择创作工作空间"} size="md">
        {loading ? <p role="status">正在读取工作空间…</p> : workspaceError ? (
          <ErrorState title="无法读取工作空间" description={workspaceError} />
        ) : workspaces.length ? (
          <div className="preview-workspace-picker">
            {workspaces.map(workspace => (
              <button type="button" key={workspace.id} onClick={() => navigate("/workspaces/" + encodeURIComponent(workspace.id) + "/" + destination)}>
                <WorkspaceIcon name="workspace-switch" size={24} variant="mono" />
                <span><strong>{workspace.name}</strong>{workspace.description && <small>{workspace.description}</small>}</span>
                <ArrowRight size={16} />
              </button>
            ))}
          </div>
        ) : (
          <EmptyState title="先创建一个工作空间" action={<Button onClick={() => navigate("/workspaces")}>管理工作空间</Button>} />
        )}
      </Modal>
      {desktopSettings && <DesktopSettingsDialog onClose={() => setDesktopSettings(false)} />}
    </PageShell>
  );
}
