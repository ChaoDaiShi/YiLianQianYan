import { useState } from "react";
import { createSecurityGrant, deleteSecurityGrant, type SecurityGrant } from "../../api/client";
import { Badge, Button, Input } from "../../components/ui";
import {
  grantPayload,
  grantWarning,
  mcpGrantError,
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
    case "mcp":
      return `MCP：${String(resource.server_id)} / ${resource.tool_name == null ? "全部工具" : String(resource.tool_name)}`;
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
    if (state.kind === "mcp") {
      const validation = mcpGrantError(state.serverId, state.toolName);
      if (validation) { setError(validation); return; }
    }
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
    <div className="settings-grant-editor" data-testid="grant-editor">
      <section aria-labelledby="current-grants-title">
        <div className="settings-inline-heading">
          <div><h4 id="current-grants-title">当前授权</h4><p>显式允许或拒绝具体资源；拒绝规则始终优先。</p></div>
          <Badge tone="default">{grants.length} 条</Badge>
        </div>
        <div className="settings-grant-list">
          {grants.length === 0 && <p className="settings-empty-copy">暂无用户授权。缺少授权的资源请求会进入逐次审批。</p>}
          {grants.map((grant) => (
            <div key={grant.id} className="settings-grant-row">
              <div className="min-w-0">
                <div className="settings-grant-meta">
                  <Badge tone={grant.effect === "allow" ? "success" : "danger"}>{grant.effect === "allow" ? "允许" : "拒绝"}</Badge>
                  <span>{grant.permission}</span>
                  <Badge tone="default">{grant.source}</Badge>
                </div>
                <p>{describeResource(grant.resource)}</p>
              </div>
              <Button variant="ghost" size="sm" onClick={() => remove(grant)} disabled={busy}>删除</Button>
            </div>
          ))}
        </div>
      </section>

      <details className="settings-disclosure settings-grant-create">
        <summary>
          <span><strong>创建资源授权</strong><small>高级操作 · 精确指定持久化匹配规则</small></span>
        </summary>
        <div className="settings-grant-form">
          <p className="settings-help">授权只配置安全网关的匹配规则，不会直接执行操作。</p>
          <div className="settings-two-column">
            <label className="settings-control-label">资源类型<select value={state.kind} onChange={(e) => setKind(e.target.value as GrantEditorKind)} className="settings-select"><option value="filesystem">文件系统</option><option value="network">网络</option><option value="process">进程</option><option value="shell">Shell</option><option value="mcp">MCP 服务 / 工具</option></select></label>
            <label className="settings-control-label">效果<select value={state.effect} onChange={(e) => update("effect", e.target.value as GrantEditorState["effect"])} className="settings-select"><option value="allow">允许</option><option value="deny">拒绝</option></select></label>
          </div>

          {state.kind === "mcp" && <>
            <Input label="Server ID" value={state.serverId} onChange={(e) => update("serverId", e.target.value)} placeholder="服务的精确 ID" />
            <Input label="工具名（可选）" value={state.toolName} onChange={(e) => update("toolName", e.target.value)} placeholder="留空表示该服务全部工具" />
          </>}

          {state.kind === "filesystem" && <>
            <label className="settings-control-label">权限<select value={state.permission} onChange={(e) => update("permission", e.target.value)} className="settings-select"><option value="filesystem.read">读取</option><option value="filesystem.write">写入</option></select></label>
            <Input label="根目录" value={state.root} onChange={(e) => update("root", e.target.value)} placeholder="例如 C:\\workspace" />
            <label className="settings-checkbox-row"><input type="checkbox" checked={state.recursive} onChange={(e) => update("recursive", e.target.checked)} />允许递归子路径</label>
          </>}

          {state.kind === "network" && <>
            <div className="settings-two-column"><label className="settings-control-label">协议<select value={state.scheme} onChange={(e) => update("scheme", e.target.value as GrantEditorState["scheme"])} className="settings-select"><option>https</option><option>http</option></select></label><Input label="端口（可选）" value={state.port} onChange={(e) => update("port", e.target.value)} placeholder="443" /></div>
            <Input label="主机或单级通配域名" value={state.host} onChange={(e) => update("host", e.target.value)} placeholder="api.example.com 或 *.example.com" />
            <div className="settings-two-column"><Input label="HTTP 方法（逗号分隔）" value={state.methods} onChange={(e) => update("methods", e.target.value)} /><label className="settings-control-label">网络区域<select value={state.zone} onChange={(e) => update("zone", e.target.value as GrantEditorState["zone"])} className="settings-select"><option value="public">Public</option><option value="loopback">Loopback</option><option value="private">Private</option></select></label></div>
          </>}

          {state.kind === "process" && <p className="settings-notice">进程控制范围固定为仅 Agent 管理的子进程；主机任意进程和显式 PID 不属于当前授权模型。</p>}
          {state.kind === "shell" && <label className="settings-checkbox-row"><input type="checkbox" checked={state.hostEscapeAcknowledged} onChange={(e) => update("hostEscapeAcknowledged", e.target.checked)} />我明确知道这会允许 Shell 逃逸工作区边界</label>}
          {warning && <p className="settings-notice settings-notice--warning">{warning}</p>}
          {error && <p className="settings-form-error" role="alert">{error}</p>}
          <Button size="sm" onClick={submit} disabled={busy}>{busy ? "处理中…" : "创建授权"}</Button>
        </div>
      </details>
    </div>
  );
}
