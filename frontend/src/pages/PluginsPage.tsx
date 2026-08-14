import { useEffect, useState } from "react";
import {
  Link, Plus, Trash2, ToggleLeft, ToggleRight, Wrench,
  Loader2, TestTube, AlertTriangle,
} from "lucide-react";
import {
  listPlugins, createMcpServer, updateMcpServer, deleteMcpServer,
  toggleMcpServer, testMcpServer,
  MCP_RUNTIME_STATUS_LABELS, mcpTransportRuntimeLabel,
  type McpServer, type PluginListResponse,
} from "../api/client";
import { PageHeader, Button, Badge, Modal, Input, EmptyState, Spinner, Panel } from "../components/ui";

export default function PluginsPage() {
  const [data, setData] = useState<PluginListResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [testingId, setTestingId] = useState<string | null>(null);

  // Add/Edit modal
  const [showModal, setShowModal] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formName, setFormName] = useState("");
  const [formTransport, setFormTransport] = useState<"stdio" | "sse">("stdio");
  const [formCommand, setFormCommand] = useState("");
  const [formArgs, setFormArgs] = useState("");
  const [formUrl, setFormUrl] = useState("");
  const [formEnv, setFormEnv] = useState("");
  const [formEnvKeyCount, setFormEnvKeyCount] = useState(0);
  const [saving, setSaving] = useState(false);
  const [testResults, setTestResults] = useState<Record<string, { ok: boolean; message: string }>>({});

  const load = async () => {
    const res = await listPlugins();
    if (res) {
      setData(res);
      setError("");
    } else {
      setError("无法连接到后端");
    }
    setLoading(false);
  };

  useEffect(() => { load(); }, []);

  const openAdd = () => {
    setEditingId(null);
    setFormName("");
    setFormTransport("stdio");
    setFormCommand("");
    setFormArgs("");
    setFormUrl("");
    setFormEnv("");
    setFormEnvKeyCount(0);
    setShowModal(true);
  };

  const openEdit = (mcp: McpServer) => {
    setEditingId(mcp.id);
    setFormName(mcp.name);
    setFormTransport(mcp.transport as "stdio" | "sse");
    setFormCommand(mcp.command || "");
    setFormArgs(mcp.args ? JSON.stringify(mcp.args) : "");
    setFormUrl(mcp.url || "");
    setFormEnv("");
    setFormEnvKeyCount(mcp.env && typeof mcp.env === "object" ? Object.keys(mcp.env).length : 0);
    setShowModal(true);
  };

  const handleSave = async () => {
    if (!formName.trim()) return;
    setSaving(true);
    const env = formEnv.trim()
      ? (() => { try { return JSON.parse(formEnv) as Record<string, string>; } catch { return {}; } })()
      : editingId ? undefined : {};
    const payload: Partial<McpServer> = {
      name: formName.trim(),
      transport: formTransport,
      command: formTransport === "stdio" ? formCommand.trim() || null : null,
      args: formArgs.trim() ? (() => { try { return JSON.parse(formArgs); } catch { return [formArgs.trim()]; } })() : [],
      url: formTransport === "sse" ? formUrl.trim() || null : null,
      ...(env === undefined ? {} : { env }),
    };
    if (editingId) {
      await updateMcpServer(editingId, payload);
    } else {
      await createMcpServer(payload);
    }
    setShowModal(false);
    setSaving(false);
    load();
  };

  const handleDelete = async (id: string, name: string) => {
    if (!window.confirm(`确定删除 MCP 服务器 "${name}" 吗？`)) return;
    await deleteMcpServer(id);
    load();
  };

  const handleToggle = async (id: string) => {
    await toggleMcpServer(id);
    load();
  };

  const handleTest = async (id: string) => {
    setTestingId(id);
    const result = await testMcpServer(id);
    if (result) setTestResults((current) => ({ ...current, [id]: result }));
    setTestingId(null);
  };

  if (loading) {
    return (
      <div className="flex justify-center py-20">
        <Spinner className="w-8 h-8" />
      </div>
    );
  }

  if (error && !data) {
    return (
      <EmptyState
        icon={<AlertTriangle className="w-12 h-12 text-[var(--warning)]" />}
        title="无法加载插件"
        description={error}
        action={<Button variant="secondary" onClick={load}>重试</Button>}
      />
    );
  }

  return (
    <div className="flex flex-col h-full">
      <PageHeader
        title="插件管理"
        description="管理内置工具、MCP 服务器与外部工具扩展"
        actions={
          <Button size="sm" onClick={openAdd}>
            <Plus className="w-4 h-4" />
            添加 MCP
          </Button>
        }
      />

      <div className="flex-1 overflow-y-auto p-6 space-y-6 scrollbar-thin">
        <Panel className="flex items-start gap-3">
          <Wrench className="w-5 h-5 text-[var(--accent)] flex-shrink-0 mt-0.5" />
          <div>
            <p className="text-sm font-medium">
              {data?.mcp_runtime_ready ? MCP_RUNTIME_STATUS_LABELS.ready : MCP_RUNTIME_STATUS_LABELS.unready}
            </p>
            <p className="text-xs text-[var(--text-muted)] mt-0.5">
              当前生产 Runtime 仅支持 stdio；sse / http 配置不会进入可运行 Runtime。
            </p>
          </div>
        </Panel>

        {/* Builtin Tools */}
        <div>
          <h3 className="text-sm font-semibold text-[var(--text-muted)] uppercase tracking-wider mb-3">
            内置工具 ({data?.builtin.length || 0})
          </h3>
          <div className="grid grid-cols-2 gap-2">
            {(data?.builtin || []).map((tool) => (
              <div
                key={tool.name}
                className="flex items-center gap-3 px-3 py-2.5 rounded-lg border border-[var(--border)] bg-[var(--panel)]/50"
              >
                <Wrench className="w-4 h-4 text-[var(--accent)] flex-shrink-0" />
                <div className="min-w-0">
                  <code className="text-sm font-mono font-medium">{tool.name}</code>
                  <p className="text-xs text-[var(--text-muted)] truncate">{tool.description}</p>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Divider */}
        <div className="flex items-center gap-3">
          <div className="flex-1 border-t border-[var(--border)]" />
          <span className="text-xs text-[var(--text-faint)] font-mono uppercase">MCP 服务器</span>
          <div className="flex-1 border-t border-[var(--border)]" />
        </div>

        {/* MCP Servers */}
        {(!data?.mcp || data.mcp.length === 0) ? (
          <EmptyState
            icon={<Link className="w-10 h-10" />}
            title="暂无 MCP 服务器"
            description="添加外部 MCP 服务器扩展 AI 能力"
            action={<Button size="sm" onClick={openAdd}><Plus className="w-4 h-4" />添加 MCP</Button>}
          />
        ) : (
          <div className="space-y-3">
            {data.mcp.map((mcp) => (
              <Panel key={mcp.id} className="group">
                <div className="flex items-start justify-between">
                  <div className="flex items-center gap-3 min-w-0">
                    <Link className="w-5 h-5 text-[var(--accent)] flex-shrink-0" />
                    <div>
                      <div className="flex items-center gap-2">
                        <h4 className="font-medium text-sm">{mcp.name}</h4>
                        {mcp.enabled ? (
                          <Badge tone="success">已启用</Badge>
                        ) : (
                          <Badge tone="default">已禁用</Badge>
                        )}
                        {!data.mcp_runtime_ready && mcp.enabled && (
                          <span className="text-[10px] text-[var(--text-faint)] italic">
                            不会进入 Agent Runtime registry
                          </span>
                        )}
                        {!mcp.enabled && (
                          <span className="text-[10px] text-[var(--text-faint)] italic">
                            不会进入 Agent Runtime registry
                          </span>
                        )}
                      </div>
                      <p className="text-xs text-[var(--text-muted)] mt-0.5 font-mono">
                        {mcp.transport === "stdio"
                          ? `stdio · ${mcp.command || "(未配置)"}`
                          : `${mcp.transport} · ${mcp.url || "(未配置)"}`}
                      </p>
                      <p className="text-[10px] text-[var(--text-faint)] mt-1">
                        {mcpTransportRuntimeLabel(mcp.transport)} · 参数 {mcp.args?.length || 0} 个 · 环境变量 {mcp.env ? Object.keys(mcp.env).length : 0} 个（值不展示）
                      </p>
                      <p className="text-[10px] text-[var(--text-faint)] mt-0.5">
                        运行状态：{mcp.runtime_status || mcpTransportRuntimeLabel(mcp.transport)}
                      </p>
                      {testResults[mcp.id] && (
                        <p className={`text-xs mt-2 ${testResults[mcp.id].ok ? "text-[var(--success)]" : "text-[var(--danger)]"}`}>
                          {testResults[mcp.id].ok ? "连接成功" : "连接失败"}：{testResults[mcp.id].message}
                        </p>
                      )}
                    </div>
                  </div>

                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                    <button
                      onClick={() => handleToggle(mcp.id)}
                      className="p-1.5 rounded hover:bg-[var(--panel-hover)] text-[var(--text-muted)] hover:text-[var(--text)]"
                      title={mcp.enabled ? "禁用" : "启用"}
                    >
                      {mcp.enabled ? <ToggleRight className="w-4 h-4 text-[var(--success)]" /> : <ToggleLeft className="w-4 h-4" />}
                    </button>
                    <button
                      onClick={() => handleTest(mcp.id)}
                      disabled={testingId === mcp.id}
                      className="p-1.5 rounded hover:bg-[var(--panel-hover)] text-[var(--text-muted)] hover:text-[var(--text)]"
                      title="测试连接"
                    >
                      {testingId === mcp.id ? (
                        <Loader2 className="w-4 h-4 animate-spin" />
                      ) : (
                        <TestTube className="w-4 h-4" />
                      )}
                    </button>
                    <button
                      onClick={() => openEdit(mcp)}
                      className="p-1.5 rounded hover:bg-[var(--panel-hover)] text-[var(--text-muted)] hover:text-[var(--text)]"
                      title="编辑"
                    >
                      <Wrench className="w-4 h-4" />
                    </button>
                    <button
                      onClick={() => handleDelete(mcp.id, mcp.name)}
                      className="p-1.5 rounded hover:bg-[var(--danger)]/20 text-[var(--text-muted)] hover:text-[var(--danger)]"
                      title="删除"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              </Panel>
            ))}
          </div>
        )}

        {/* Runtime status notice */}
        {data && !data.mcp_runtime_ready && (
          <div className="flex items-start gap-3 px-4 py-3 rounded-xl border border-[var(--warning)]/30 bg-[var(--warning)]/10">
            <AlertTriangle className="w-5 h-5 text-[var(--warning)] flex-shrink-0 mt-0.5" />
            <div>
              <p className="text-sm font-medium text-[var(--warning)]">{MCP_RUNTIME_STATUS_LABELS.unready}</p>
              <p className="text-xs text-[var(--text-muted)] mt-0.5">
                当前启用配置不会进入 Agent Runtime registry，请先确认 stdio Runtime 已就绪。
              </p>
            </div>
          </div>
        )}
      </div>

      {/* Add/Edit MCP Modal */}
      <Modal
        open={showModal}
        onClose={() => setShowModal(false)}
        title={editingId ? "编辑 MCP 服务器" : "添加 MCP 服务器"}
        footer={
          <>
            <Button variant="secondary" onClick={() => setShowModal(false)}>取消</Button>
            <Button onClick={handleSave} disabled={saving}>
              {saving ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
              {editingId ? "保存" : "添加"}
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          <Input
            label="名称"
            value={formName}
            onChange={(e) => setFormName(e.target.value)}
            placeholder="例如：Browser MCP"
          />
          <div>
            <label className="block text-sm font-medium mb-1">传输方式</label>
            <select
              value={formTransport}
              onChange={(e) => setFormTransport(e.target.value as "stdio" | "sse")}
              className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
            >
              <option value="stdio">Stdio (命令行)</option>
              <option value="sse">SSE (HTTP)</option>
            </select>
          </div>

          {formTransport === "stdio" ? (
            <>
              <Input
                label="命令"
                value={formCommand}
                onChange={(e) => setFormCommand(e.target.value)}
                placeholder="例如：npx @anthropic/mcp-server"
              />
              <Input
                label="参数 (JSON 数组)"
                value={formArgs}
                onChange={(e) => setFormArgs(e.target.value)}
                placeholder='例如：["--port", "8080"]'
              />
            </>
          ) : (
            <Input
              label="URL"
              value={formUrl}
              onChange={(e) => setFormUrl(e.target.value)}
              placeholder="例如：http://127.0.0.1:3001/sse"
            />
          )}

          <div>
            <label className="block text-sm font-medium mb-1">环境变量 (JSON 对象；留空则保留现有值)</label>
            {editingId && formEnvKeyCount > 0 && (
              <p className="text-xs text-[var(--text-muted)] mb-1">已配置 {formEnvKeyCount} 个环境变量键；为保护 Secret，不回显值。</p>
            )}
            <textarea
              value={formEnv}
              onChange={(e) => setFormEnv(e.target.value)}
              rows={3}
              placeholder='{"API_KEY": "sk-..."}'
              className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] placeholder:text-[var(--text-faint)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none font-mono"
            />
          </div>

          {!data?.mcp_runtime_ready && (
            <p className="text-xs text-[var(--text-muted)] italic">
              配置保存后将持久化到数据库。MCP 运行时注入将在后续版本中接入。
            </p>
          )}
        </div>
      </Modal>
    </div>
  );
}
