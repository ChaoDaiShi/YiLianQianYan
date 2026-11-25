import { useEffect, useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import PageShell from "../../components/layout/PageShell";
import { ErrorState } from "../../components/ui";
import { getWorkspace, type Workspace } from "../../api/workspaces";
import WorkspaceSectionNav, { type WorkspaceSection } from "./WorkspaceSectionNav";

export default function WorkspaceSectionShell({ workspaceId, active, children }: { workspaceId: string; active: WorkspaceSection; children: (workspace: Workspace) => ReactNode }) {
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let current = true;
    setWorkspace(null); setError("");
    void getWorkspace(workspaceId).then(result => {
      if (!current) return;
      if (result.ok) setWorkspace(result.data);
      else setError(result.error);
    });
    return () => { current = false; };
  }, [workspaceId, revision]);
  return <PageShell title={<div className="workspace-breadcrumb"><Link to={workspace?.status === "archived" ? "/workspaces?view=archived" : "/workspaces"}>工作空间</Link><span>/</span><h1>{workspace?.name ?? "工作空间"}</h1></div>}
    variant="workspace" maxWidth="none" className="workspace-preview-page">
    {error ? <ErrorState className="m-4" title="工作空间暂时无法加载" description={error} onRetry={() => setRevision(value => value + 1)} /> : !workspace ? <p role="status" className="p-4">正在读取工作空间…</p> : <>
      <WorkspaceSectionNav workspaceId={workspaceId} active={active} />
      {workspace.status === "archived" && <p role="status" className="px-5 pt-2 text-xs text-[var(--text-secondary)]">此空间已归档，可查看已保存记录。</p>}
      {children(workspace)}
    </>}
  </PageShell>;
}
