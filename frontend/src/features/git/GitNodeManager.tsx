import { useCallback, useEffect, useRef, useState } from "react";
import { FileDiff, GitBranch, RefreshCw, FolderGit2, GitCommitHorizontal } from "lucide-react";
import { listWorkspaces, type Workspace } from "../../api/workspaces";
import { getWorkspaceGit, getWorkspaceGitDiff, type GitFileChange, type WorkspaceGitDiff, type WorkspaceGitStatus } from "../../api/workspaceGit";
import { Modal } from "../../components/ui";
import { Link } from "react-router-dom";
import { gitChangeLabel, gitTrackingLabel } from "./gitPresentation";
import "./gitNodeManager.css";

export interface GitNodeManagerProps {
  visible: boolean;
  /** Omit for local selection. Null means no workspace has been selected. */
  workspaceId?: string | null;
  onWorkspaceChange?: (id: string | null) => void;
}
interface DiffSelection { file: GitFileChange; staged: boolean }

export default function GitNodeManager({ visible, workspaceId, onWorkspaceChange }: GitNodeManagerProps) {
  const [localWorkspace, setLocalWorkspace] = useState<string | null>(null);
  const selectedWorkspace = workspaceId === undefined ? localWorkspace : workspaceId;
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspaceError, setWorkspaceError] = useState("");
  const [status, setStatus] = useState<WorkspaceGitStatus | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [selection, setSelection] = useState<DiffSelection | null>(null);
  const [diff, setDiff] = useState<WorkspaceGitDiff | null>(null);
  const [diffError, setDiffError] = useState("");
  const requestVersion = useRef(0);
  const activeRequest = useRef<number | null>(null);

  useEffect(() => {
    if (!visible) return;
    let active = true;
    void listWorkspaces().then(result => {
      if (!active) return;
      if (result.ok) { setWorkspaces(result.data); setWorkspaceError(""); }
      else setWorkspaceError(result.error);
    });
    return () => { active = false; };
  }, [visible]);

  const refresh = useCallback(async () => {
    if (!visible || !selectedWorkspace || document.hidden || activeRequest.current !== null) return;
    const version = ++requestVersion.current;
    activeRequest.current = version;
    setBusy(true);
    const result = await getWorkspaceGit(selectedWorkspace);
    if (version !== requestVersion.current) return;
    activeRequest.current = null;
    if (result.ok) { setStatus(result.data); setError(""); }
    else { setStatus(null); setError(result.error); }
    setBusy(false);
  }, [selectedWorkspace, visible]);

  useEffect(() => {
    setStatus(null); setSelection(null); setError(""); setBusy(false);
    if (!visible || !selectedWorkspace) return;
    void refresh();
    const timer = window.setInterval(() => void refresh(), 5000);
    const resume = () => { if (!document.hidden) void refresh(); };
    document.addEventListener("visibilitychange", resume);
    return () => { requestVersion.current++; activeRequest.current = null; window.clearInterval(timer); document.removeEventListener("visibilitychange", resume); };
  }, [refresh, visible]);

  useEffect(() => {
    setDiff(null); setDiffError("");
    if (!selection || !visible || !selectedWorkspace) return;
    let active = true;
    void getWorkspaceGitDiff(selection.file.path, selection.staged, selectedWorkspace).then(result => {
      if (!active) return;
      if (result.ok) setDiff(result.data); else setDiffError(result.error);
    });
    return () => { active = false; };
  }, [selection, selectedWorkspace, visible]);

  function selectWorkspace(id: string) {
    const next = id || null;
    if (workspaceId === undefined) setLocalWorkspace(next);
    onWorkspaceChange?.(next);
  }
  if (!visible) return null;
  return <aside className="git-node-manager" aria-label="Git 文件变更">
    <header className="git-node-header"><div><FolderGit2 size={17}/><h2>Git 节点</h2></div><button type="button" className="git-refresh" onClick={() => void refresh()} disabled={busy || !selectedWorkspace} aria-label="刷新 Git 文件变更" title="刷新文件变更"><RefreshCw size={15} className={busy ? "animate-spin" : ""}/></button></header>
    <label className="git-workspace-label">工作空间<select aria-label="Git 工作空间" value={selectedWorkspace ?? ""} onChange={event => selectWorkspace(event.target.value)} disabled={workspaceId !== undefined && !onWorkspaceChange}><option value="">选择本地工作空间</option>{workspaces.filter(workspace => workspace.root_path?.trim()).map(workspace => <option key={workspace.id} value={workspace.id}>{workspace.name} · {workspace.root_path}{workspace.status === "archived" ? " · 已归档" : ""}</option>)}</select></label>
    {workspaceError && <p className="git-node-error" role="alert">工作空间列表加载失败：{workspaceError}</p>}
    {!selectedWorkspace ? <div className="git-node-empty"><FolderGit2 size={24}/><p>选择你要查看的本地工作空间。</p><p>尚未添加目录时，可先<Link to="/workspaces" className="text-[var(--accent-primary)] underline">管理工作空间</Link>。</p></div> : error ? <p className="git-node-error" role="alert">{error}</p> : !status ? <p className="git-node-empty" role="status">正在读取 Git 工作区…</p> : !status.is_repository ? <div className="git-node-empty"><FolderGit2 size={24}/><p>该工作空间不是 Git 仓库</p><span className="git-root-path">{status.root}</span></div> : <>
      <div className="git-repository"><div className="git-branch"><GitBranch size={15}/><strong>{status.branch ?? "分离 HEAD"}</strong></div><p className="git-root-path" title={status.root}>{status.root}</p><p className="git-commit"><GitCommitHorizontal size={13}/>{status.head ? <code title={status.head}>{status.head.slice(0, 10)}</code> : <span>尚无提交</span>}</p><p className="git-tracking">{gitTrackingLabel(status.ahead, status.behind)}</p></div>
      <div className="git-files-title"><span>文件变更</span><span>{status.files.length}</span></div>
      <div className="git-file-list">{status.files.length === 0 ? <p className="git-node-empty">工作区干净，没有文件变更</p> : status.files.map(file => <article className="git-file" key={file.path}>
        <div className="git-file-name"><FileDiff size={14}/><span title={file.path}>{file.path}</span></div>
        {file.original_path && <p className="git-rename-path" title={file.original_path}>{file.original_path} → {file.path}</p>}
        <div className="git-file-actions">{file.staged && <button type="button" data-git-area="staged" onClick={() => setSelection({ file, staged: true })} aria-label={`查看已暂存差异 ${file.path}`}><span className="git-change-code">{file.index_status}</span>已暂存 · {gitChangeLabel(file, true)}</button>}{(file.unstaged || file.untracked) && <button type="button" data-git-area="worktree" onClick={() => setSelection({ file, staged: false })} aria-label={`查看${file.untracked ? "未跟踪内容" : "工作区差异"} ${file.path}`}><span className="git-change-code">{file.worktree_status}</span>{file.untracked ? "未跟踪 · 查看内容" : `工作区 · ${gitChangeLabel(file, false)}`}</button>}</div>
      </article>)}</div>
    </>}
    <p className="git-node-footnote">显示本地 Git 状态，上游计数基于本地引用。</p>
    <Modal open={selection !== null} onClose={() => setSelection(null)} title={selection ? `${selection.file.untracked ? "未跟踪文件" : selection.staged ? "已暂存差异" : "工作区差异"} · ${selection.file.path}` : "文件差异"} size="xl">
      {diffError ? <p className="git-node-error" role="alert">{diffError}</p> : !diff ? <p role="status">正在读取文件差异…</p> : <div className="git-diff-view">{diff.untracked && <p className="git-diff-note">未跟踪文件的当前内容</p>}{diff.truncated && <p className="git-diff-note" role="status">内容超过 1 MiB，当前显示前 1 MiB。</p>}{diff.binary ? <p className="git-diff-note">该文件包含二进制内容或非 UTF-8 文本，无法显示文本预览。</p> : diff.content ? <pre aria-label="文件差异内容">{diff.content}</pre> : <p className="git-diff-note">当前没有文本差异。</p>}</div>}
    </Modal>
  </aside>;
}
