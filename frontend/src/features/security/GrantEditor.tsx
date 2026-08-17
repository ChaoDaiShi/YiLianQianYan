import { useState } from "react";
import { createSecurityGrant, deleteSecurityGrant, type SecurityGrant } from "../../api/client";
import { Badge, Button, Input } from "../../components/ui";
import {
  grantPayload,
  grantWarning,
  initialGrantEditorState,
  type GrantEditorKind,
  type GrantEditorState,
} from "./grantEditorModel";

interface GrantEditorProps {
  grants: SecurityGrant[];
  onChanged: () => Promise<void>;
}

function describeResource(resource: Record<string, unknown>): string {
  switch (resource.type) {
    case "filesystem":
      return `文件：${String(resource.root)}${resource.recursive ? "（递归）" : ""}`;
    case "network":
      return `网络：${String(resource.scheme ?? "*")}://${String(resource.host)}${resource.port ? `:${String(resource.port)}` : ""} [${Array.isArray(resource.methods) ? resource.methods.join(", ") : ""}]`;
    case "process":
      return `进程：${String((resource.scope as Record<string, unknown> | undefined)?.kind ?? "unknown")}`;
    case "shell":
      return `Shell：工作区外${resource.host_escape_acknowledged ? "已确认" : "未确认"}`;
    default:
      return "未知资源";
  }
}

export function GrantEditor({ grants, onChanged }: GrantEditorProps) {
  const [state, setState] = useState<GrantEditorState>(initialGrantEditorState);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const update = <K extends keyof GrantEditorState>(key: K, value: GrantEditorState[K]) =>
    setState((current) => ({ ...current, [key]: value }));

  const setKind = (kind: GrantEditorKind) => {
    const permission = kind === "filesystem" ? "filesystem.read" : initialGrantEditorState.permission;
    setState((current) => ({ ...current, kind, permission }));
  };

  const submit = async () => {
    setError(null);
    const payload = grantPayload(state);
    if (state.kind === "filesystem" && !state.root.trim()) {
      setError("文件系统根目录不能为空");
      return;
    }
    if (state.kind === "network" && (!state.host.trim() || state.host.trim() === "*")) {
      setError("网络主机必须是明确主机或单级通配域名，不能使用 *");
      return;
    }
    setBusy(true);
    const result = await createSecurityGrant(payload);
    setBusy(false);
    if (!result) {
      setError("授权创建失败，请检查后端校验或连接状态");
      return;
    }
    await onChanged();
  };

  const remove = async (grant: SecurityGrant) => {
    if (!window.confirm("确认删除这条授权？删除后网关将立即重新评估请求。")) return;
    setBusy(true);
    const result = await deleteSecurityGrant(grant.id);
    setBusy(false);
    if (!result) {
      setError("授权删除失败");
      return;
    }
    await onChanged();
  };

  const warning = grantWarning(state);

  return (
    <div className="space-y-4">
      <div className="rounded-lg border border-[var(--border)] p-3 space-y-3">
        <div>
          <h4 className="text-sm font-semibold">创建资源授权</h4>
          <p className="text-xs text-[var(--text-muted)] mt-1">授权只配置安全网关的匹配规则，不会直接执行操作。</p>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <label className="text-xs">资源类型<select value={state.kind} onChange={(e) => setKind(e.target.value as GrantEditorKind)} className="mt-1 w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-2 py-2 text-sm text-[var(--text)]"><option value="filesystem">文件系统</option><option value="network">网络</option><option value="process">进程</option><option value="shell">Shell</option></select></label>
          <label className="text-xs">效果<select value={state.effect} onChange={(e) => update("effect", e.target.value as GrantEditorState["effect"])} className="mt-1 w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-2 py-2 text-sm text-[var(--text)]"><option value="allow">允许</option><option value="deny">拒绝</option></select></label>
        </div>

        {state.kind === "filesystem" && <>
          <label className="text-xs">权限<select value={state.permission} onChange={(e) => update("permission", e.target.value)} className="mt-1 w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-2 py-2 text-sm text-[var(--text)]"><option value="filesystem.read">读取</option><option value="filesystem.write">写入</option></select></label>
          <Input label="根目录" value={state.root} onChange={(e) => update("root", e.target.value)} placeholder="例如 C:\\workspace" />
          <label className="flex items-center gap-2 text-xs"><input type="checkbox" checked={state.recursive} onChange={(e) => update("recursive", e.target.checked)} />允许递归子路径</label>
        </>}

        {state.kind === "network" && <>
          <div className="grid grid-cols-2 gap-3"><label className="text-xs">协议<select value={state.scheme} onChange={(e) => update("scheme", e.target.value as GrantEditorState["scheme"])} className="mt-1 w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-2 py-2 text-sm text-[var(--text)]"><option>https</option><option>http</option></select></label><Input label="端口（可选）" value={state.port} onChange={(e) => update("port", e.target.value)} placeholder="443" /></div>
          <Input label="主机或单级通配域名" value={state.host} onChange={(e) => update("host", e.target.value)} placeholder="api.example.com 或 *.example.com" />
          <div className="grid grid-cols-2 gap-3"><Input label="HTTP 方法（逗号分隔）" value={state.methods} onChange={(e) => update("methods", e.target.value)} /><label className="text-xs">网络区域<select value={state.zone} onChange={(e) => update("zone", e.target.value as GrantEditorState["zone"])} className="mt-1 w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-2 py-2 text-sm text-[var(--text)]"><option value="public">Public</option><option value="loopback">Loopback</option><option value="private">Private</option></select></label></div>
        </>}

        {state.kind === "process" && <p className="rounded-md border border-[var(--border)] px-2 py-2 text-xs text-[var(--text-muted)]">进程控制范围固定为仅 Agent 管理的子进程；主机任意进程和显式 PID 不属于 v0.8 授权模型。</p>}

        {state.kind === "shell" && <label className="flex items-center gap-2 text-xs"><input type="checkbox" checked={state.hostEscapeAcknowledged} onChange={(e) => update("hostEscapeAcknowledged", e.target.checked)} />我明确知道这会允许 Shell 逃逸工作区边界</label>}
        {warning && <p className="rounded-md border border-[var(--warning)]/40 bg-[var(--warning)]/10 px-2 py-2 text-xs text-[var(--warning)]">{warning}</p>}
        {error && <p className="text-xs text-[var(--danger)]">{error}</p>}
        <Button size="sm" onClick={submit} disabled={busy}>{busy ? "处理中…" : "创建授权"}</Button>
      </div>

      <div className="space-y-2">
        <h4 className="text-sm font-semibold">当前授权（{grants.length}）</h4>
        {grants.length === 0 && <p className="text-xs text-[var(--text-muted)]">暂无用户授权；缺少授权的资源请求会进入审批。</p>}
        {grants.map((grant) => <div key={grant.id} className="flex items-start justify-between gap-3 rounded-lg border border-[var(--border)] px-3 py-2"><div className="min-w-0"><div className="flex items-center gap-2"><Badge tone={grant.effect === "allow" ? "success" : "danger"}>{grant.effect === "allow" ? "允许" : "拒绝"}</Badge><span className="text-xs font-medium">{grant.permission}</span><Badge tone="default">{grant.source}</Badge></div><p className="mt-1 break-all text-xs text-[var(--text-muted)]">{describeResource(grant.resource)}</p></div><Button variant="ghost" size="sm" onClick={() => remove(grant)} disabled={busy}>删除</Button></div>)}
      </div>
    </div>
  );
}
