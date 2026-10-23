import { useCallback, useRef, useState } from "react";
import { Button, Input, Modal, Select, Textarea } from "../../components/ui";
import { requestResult } from "../../core/api/http";
import WorkflowRunInspector from "../../components/workflow/WorkflowRunInspector";
import type { WorkflowRunRecord } from "../../api/workflows";
import type { WorkspaceGitStatus } from "../../api/workspaceGit";

export type CodeAction = "stage" | "unstage" | "commit" | "push" | "lsp";
export default function CodeActionDialog({ workspaceId, status, action: initialAction, paths, onClose, onCompleted }: {
  workspaceId: string; status: WorkspaceGitStatus; action: CodeAction; paths: string[]; onClose: () => void; onCompleted: () => void;
}) {
  const [action, setAction] = useState<CodeAction>(initialAction);
  const [files, setFiles] = useState(paths.join("\n"));
  const [message, setMessage] = useState("");
  const [remote, setRemote] = useState("origin");
  const [branch, setBranch] = useState(status.branch ?? "");
  const [serverPath, setServerPath] = useState("");
  const [serverArgs, setServerArgs] = useState("[]");
  const [languageId, setLanguageId] = useState("typescript");
  const [lspAction, setLspAction] = useState("definition");
  const [line, setLine] = useState(1);
  const [character, setCharacter] = useState(1);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [runId, setRunId] = useState<string | null>(null);
  const refreshed = useRef<string | null>(null);
  const loaded = useCallback((run: WorkflowRunRecord) => {
    if (["completed", "failed", "cancelled"].includes(run.status) && refreshed.current !== run.run_id) {
      refreshed.current = run.run_id; onCompleted();
    }
  }, [onCompleted]);
  async function submit() {
    setError("");
    let payload: Record<string, unknown>;
    if (action === "lsp") {
      let args: unknown;
      try { args = JSON.parse(serverArgs); if (!Array.isArray(args) || !args.every(value => typeof value === "string")) throw new Error("服务器参数应为 JSON 字符串数组"); }
      catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); return; }
      if (!serverPath.trim() || !files.trim() || line < 1 || character < 1) { setError("请填写服务器路径、源码文件及有效行列位置"); return; }
      payload = { workspace_id: workspaceId, server_path: serverPath, server_args: args, language_id: languageId, path: files.trim(), action: lspAction, line: line - 1, character: character - 1 };
    } else payload = { workspace_id: workspaceId, expected_root: status.root, expected_head: status.head, action, paths: files.split(/\r?\n/).map(value => value.trim()).filter(Boolean), message: message || undefined, remote: action === "push" ? remote : undefined, branch: action === "push" ? branch : undefined };
    setBusy(true);
    const result = await requestResult<{ run_id: string }>("POST", action === "lsp" ? "/api/workspace-code/lsp" : "/api/workspace-git/actions", payload);
    setBusy(false);
    if (result.ok) setRunId(result.data.run_id); else setError(result.error);
  }
  return <Modal open onClose={onClose} title={action === "lsp" ? "语言服务" : "Git 操作"} size="lg" footer={<><Button variant="secondary" onClick={onClose}>关闭</Button>{!runId && <Button loading={busy} disabled={busy} onClick={() => void submit()}>提交操作</Button>}</>}>
    {runId ? <WorkflowRunInspector key={runId} runId={runId} onClose={() => setRunId(null)} onRunLoaded={loaded} /> : <div className="space-y-4">
      {error && <p role="alert" className="text-sm text-[var(--danger)]">{error}</p>}
      <p className="text-xs text-[var(--text-secondary)] break-all">{status.root} · {status.branch ?? "分离 HEAD"}</p>
      <Select label="操作" value={action} onChange={event => setAction(event.target.value as CodeAction)} options={[{value:"stage",label:"暂存指定文件"},{value:"unstage",label:"取消暂存指定文件"},{value:"commit",label:"提交暂存区"},{value:"push",label:"推送当前 HEAD"},{value:"lsp",label:"查询语言服务"}]} />
      {(action === "stage" || action === "unstage") && <Textarea label="文件相对路径（每行一个）" value={files} onChange={event => setFiles(event.target.value)} rows={4} />}
      {action === "commit" && <Textarea label="提交说明" value={message} onChange={event => setMessage(event.target.value)} rows={3} placeholder="feat(scope): describe the change" />}
      {action === "push" && <><Input label="已有远端名称" value={remote} onChange={event => setRemote(event.target.value)} /><Input label="目标分支" value={branch} onChange={event => setBranch(event.target.value)} /></>}
      {action === "lsp" && <><Input label="已安装语言服务器路径" value={serverPath} onChange={event => setServerPath(event.target.value)} placeholder="完整可执行文件路径" /><Input label="启动参数（JSON 数组）" value={serverArgs} onChange={event => setServerArgs(event.target.value)} /><Input label="语言标识" value={languageId} onChange={event => setLanguageId(event.target.value)} /><Input label="源码相对路径" value={files} onChange={event => setFiles(event.target.value)} /><Select label="查询内容" value={lspAction} onChange={event => setLspAction(event.target.value)} options={[{value:"definition",label:"定义"},{value:"references",label:"引用"},{value:"diagnostics",label:"诊断"}]} /><div className="grid grid-cols-2 gap-3"><Input label="行（从 1 开始）" type="number" min={1} value={line} onChange={event => setLine(Number(event.target.value))} /><Input label="列（从 1 开始）" type="number" min={1} value={character} onChange={event => setCharacter(Number(event.target.value))} /></div></>}
    </div>}
  </Modal>;
}
