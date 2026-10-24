import { useCallback, useEffect, useState } from "react";
import { ChevronDown, ChevronRight, Download, FileText, Folder, GitBranch, RefreshCw } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { listWorkspaceFiles, readWorkspaceFile, type WorkspaceFileEntry, type WorkspaceTextFile } from "../../api/workspaceFiles";
import { getWorkspaceGit, type WorkspaceGitStatus } from "../../api/workspaceGit";
import { Button, EmptyState, ErrorState } from "../../components/ui";

interface Props {
  workspaceId: string;
  rootPath?: string | null;
  onEdit: () => void;
  onShowChanges: () => void;
}

export default function WorkspaceFilesPanel({ workspaceId, rootPath, onEdit, onShowChanges }: Props) {
  const [directories, setDirectories] = useState<Record<string, WorkspaceFileEntry[]>>({});
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState<Set<string>>(new Set());
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [selectedPath, setSelectedPath] = useState("");
  const [file, setFile] = useState<WorkspaceTextFile | null>(null);
  const [fileError, setFileError] = useState("");
  const [reading, setReading] = useState(false);
  const [git, setGit] = useState<WorkspaceGitStatus | null>(null);
  const [gitError, setGitError] = useState("");
  const [revision, setRevision] = useState(0);

  const loadDirectory = useCallback(async (path: string) => {
    setLoading(current => new Set(current).add(path));
    setErrors(current => ({ ...current, [path]: "" }));
    const result = await listWorkspaceFiles(workspaceId, path);
    if (result.ok) {
      setDirectories(current => ({ ...current, [path]: result.data.entries }));
      if (!path) {
        const readme = result.data.entries.find(entry => /^readme\.md$/i.test(entry.name) && entry.kind === "file");
        if (readme) setSelectedPath(current => current || readme.path);
      }
    } else setErrors(current => ({ ...current, [path]: result.error }));
    setLoading(current => { const next = new Set(current); next.delete(path); return next; });
  }, [workspaceId]);

  useEffect(() => {
    if (!rootPath) return;
    void loadDirectory("");
    let active = true;
    void getWorkspaceGit(workspaceId).then(result => {
      if (!active) return;
      if (result.ok) { setGit(result.data); setGitError(""); }
      else { setGit(null); setGitError(result.error); }
    });
    return () => { active = false; };
  }, [workspaceId, rootPath, revision, loadDirectory]);

  useEffect(() => {
    if (!selectedPath) return;
    let active = true;
    setFile(null); setFileError(""); setReading(true);
    void readWorkspaceFile(workspaceId, selectedPath).then(result => {
      if (!active) return;
      if (result.ok) setFile(result.data); else setFileError(result.error);
      setReading(false);
    });
    return () => { active = false; };
  }, [workspaceId, selectedPath, revision]);

  function toggle(entry: WorkspaceFileEntry) {
    setExpanded(current => {
      const next = new Set(current);
      if (next.has(entry.path)) next.delete(entry.path); else next.add(entry.path);
      return next;
    });
    if (!directories[entry.path] && !loading.has(entry.path)) void loadDirectory(entry.path);
  }

  function download() {
    if (!file) return;
    const url = URL.createObjectURL(new Blob([file.content], { type: "text/plain;charset=utf-8" }));
    const link = document.createElement("a"); link.href = url; link.download = file.name; link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function tree(path: string, depth: number) {
    return <ul className="workspace-file-branch" aria-label={path ? `${path} 的文件` : "项目文件"}>
      {(directories[path] ?? []).map(entry => <li key={entry.path}>
        <button type="button" className={`workspace-file-entry ${selectedPath === entry.path ? "is-selected" : ""}`} style={{ paddingLeft: `${12 + depth * 16}px` }}
          title={entry.path} aria-expanded={entry.kind === "directory" ? expanded.has(entry.path) : undefined}
          aria-current={selectedPath === entry.path ? "true" : undefined}
          disabled={entry.kind === "link" || entry.kind === "other"}
          onClick={() => entry.kind === "directory" ? toggle(entry) : setSelectedPath(entry.path)}>
          {entry.kind === "directory" ? <>{expanded.has(entry.path) ? <ChevronDown size={14} /> : <ChevronRight size={14} />}<Folder size={17} className="workspace-tree-folder" /></> : <><span className="workspace-tree-spacer" /><FileText size={17} /></>}
          <span>{entry.name}{entry.kind === "link" ? "（链接）" : ""}</span>
        </button>
        {entry.kind === "directory" && expanded.has(entry.path) && <>
          {loading.has(entry.path) && <p className="workspace-tree-note" role="status">正在读取…</p>}
          {errors[entry.path] && <p className="workspace-inline-error" role="alert">{errors[entry.path]}<button type="button" onClick={() => void loadDirectory(entry.path)}>重新读取</button></p>}
          {directories[entry.path]?.length === 0 && <p className="workspace-tree-note">空文件夹</p>}
          {tree(entry.path, depth + 1)}
        </>}
      </li>)}
    </ul>;
  }

  if (!rootPath) return <EmptyState icon={<Folder size={24} />} title="为空间选择一个本地目录" description="绑定目录后，可以在这里浏览项目文件和阅读文本内容。" action={<Button onClick={onEdit}>绑定本地目录</Button>} />;

  return <div className="workspace-files-layout">
    <aside className="workspace-file-tree">
      <header><h2>项目文件</h2><Button variant="ghost" size="sm" iconOnly title="刷新项目文件" aria-label="刷新项目文件" onClick={() => { setDirectories({}); setExpanded(new Set()); setRevision(current => current + 1); }}><RefreshCw size={15} /></Button></header>
      <div className="workspace-tree-scroll scrollbar-thin">
        {loading.has("") && <p className="workspace-tree-note" role="status">正在读取项目文件…</p>}
        {errors[""] && <ErrorState title="文件目录无法读取" description={errors[""]} onRetry={() => void loadDirectory("")} />}
        {directories[""]?.length === 0 && <p className="workspace-tree-note">这个目录还没有文件。</p>}
        {tree("", 0)}
      </div>
      <p className="workspace-file-root" title={rootPath}>{rootPath}</p>
    </aside>
    <section className="workspace-file-preview" aria-label="文件预览">
      <header className="workspace-preview-toolbar"><div className="workspace-preview-mode"><span>预览</span><button type="button" onClick={onShowChanges}>修改对比</button></div>{file && <Button variant="ghost" size="sm" onClick={download}><Download size={15} />下载文件</Button>}</header>
      <div className="workspace-preview-content scrollbar-thin">
        {reading ? <p className="workspace-tree-note" role="status">正在读取文件…</p> : fileError ? <ErrorState title="文件暂不能预览" description={fileError} /> : !file ? <EmptyState icon={<FileText size={24} />} title="选择一个文件" description="从左侧项目文件中选择文本或 Markdown 文件阅读。" /> : <>
          <p className="workspace-preview-filename"><FileText size={14} />{file.path}<span>{file.size < 1024 ? `${file.size} B` : `${(file.size / 1024).toFixed(1)} KB`}</span></p>
          {/\.(md|markdown)$/i.test(file.name) ? <article className="workspace-markdown"><ReactMarkdown remarkPlugins={[remarkGfm]}>{file.content}</ReactMarkdown></article> : <pre className="workspace-text-preview">{file.content || "（空文件）"}</pre>}
        </>}
      </div>
      <footer className="workspace-git-summary"><GitBranch size={17} /><span>{gitError ? "Git 状态读取失败" : !git ? "正在读取 Git 状态…" : git.is_repository ? `Git：${git.files.length} 个文件变更${git.branch ? ` · ${git.branch}` : ""}` : "当前目录尚未初始化 Git"}</span><button type="button" onClick={onShowChanges}>查看差异<ChevronRight size={14} /></button>{gitError && <span className="sr-only">{gitError}</span>}</footer>
    </section>
  </div>;
}
