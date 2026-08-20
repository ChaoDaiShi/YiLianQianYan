import { useEffect, useState } from "react";
import {
  Link, Plus, Trash2, ToggleLeft, ToggleRight, Wrench,
  Loader2, TestTube, RefreshCw, Eye, ChevronDown, ChevronRight,
} from "lucide-react";
import {
  listPlugins, createMcpServer, updateMcpServer, deleteMcpServer,
  toggleMcpServer, testMcpServer,
  listSubagents,
  type McpServer, type PluginListResponse, type SubagentMetadata,
} from "../api/client";
import {
  mcpTransportRuntimeLabel,
  MCP_RUNTIME_STATUS_LABELS,
  MCP_RUNTIME_STATUS_TONES,
  EXTERNAL_PROMPT_WARNING,
  resourcePreview,
  promptMessageText,
  listMcpRuntimeTools,
  listMcpResources,
  listMcpResourceTemplates,
  listMcpPrompts,
  readMcpResource,
  getMcpPrompt,
  refreshMcpRuntimeServer,
  type McpRuntimeTool,
  type McpResourceDescriptor,
  type McpResourceTemplate,
  type McpPromptDescriptor,
  type McpResourceReadResponse,
  type McpPromptGetResponse,
  type McpRuntimeStatus,
} from "../api/mcpRuntime";
import { CapabilityStatusBadge } from "../components/capabilities";
import { Badge, Button, EmptyState, ErrorState, Input, Modal, PageHeader, Panel, Skeleton, Spinner } from "../components/ui";

type DetailTab = "tools" | "resources" | "prompts";

interface DetailState {
  tools: McpRuntimeTool[] | null;
  resources: McpResourceDescriptor[] | null;
  templates: McpResourceTemplate[] | null;
  prompts: McpPromptDescriptor[] | null;
  loading: boolean;
  error: string;
}

const EMPTY_DETAIL: DetailState = {
  tools: null,
  resources: null,
  templates: null,
  prompts: null,
  loading: false,
  error: "",
};

function runtimeStatusTone(status: string): "default" | "success" | "warning" | "danger" | "info" {
  return MCP_RUNTIME_STATUS_TONES[(status as McpRuntimeStatus)] ?? "default";
}

function runtimeStatusLabel(status: string): string {
  return MCP_RUNTIME_STATUS_LABELS[(status as McpRuntimeStatus)] ?? status;
}

export default function PluginsPage() {
  const [data, setData] = useState<PluginListResponse | null>(null);
  const [subagents, setSubagents] = useState<SubagentMetadata[]>([]);
  const [subagentError, setSubagentError] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [testingId, setTestingId] = useState<string | null>(null);

  // Add/Edit modal
  const [showModal, setShowModal] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formName, setFormName] = useState("");
  const [formTransport, setFormTransport] = useState<"stdio" | "streamable_http">("stdio");
  const [formCommand, setFormCommand] = useState("");
  const [formArgs, setFormArgs] = useState("");
  const [formUrl, setFormUrl] = useState("");
  const [formEnv, setFormEnv] = useState("");
  const [formEnvKeyCount, setFormEnvKeyCount] = useState(0);
  const [saving, setSaving] = useState(false);
  const [testResults, setTestResults] = useState<Record<string, { ok: boolean; message: string }>>({});

  // Server detail (Tools / Resources / Prompts)
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<DetailTab>("tools");
  const [detail, setDetail] = useState<Record<string, DetailState>>({});

  // Resource preview
  const [resourcePreviewState, setResourcePreviewState] = useState<{
    resource: McpResourceDescriptor;
    result: McpResourceReadResponse | null;
    loading: boolean;
    error: string;
  } | null>(null);

  // Prompt preview
  const [promptPreviewState, setPromptPreviewState] = useState<{
    serverId: string;
    prompt: McpPromptDescriptor;
    formValues: Record<string, string>;
    result: McpPromptGetResponse | null;
    loading: boolean;
    error: string;
  } | null>(null);

  const load = async () => {
    setLoading(true);
    setError("");
    setSubagentError("");
    try {
      const [res, subagentRes] = await Promise.all([listPlugins(), listSubagents()]);
      if (res) setData(res);
      else setError("无法连接到后端");
      if (subagentRes) setSubagents(subagentRes);
      else setSubagentError("无法加载 Subagent metadata");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
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
    setFormTransport(mcp.transport === "stdio" ? "stdio" : "streamable_http");
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
      url: formTransport === "streamable_http" ? formUrl.trim() || null : null,
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

  const handleRefresh = async (id: string) => {
    const d = detail[id] ?? EMPTY_DETAIL;
    setDetail((current) => ({ ...current, [id]: { ...d, loading: true, error: "" } }));
    const refreshed = await refreshMcpRuntimeServer(id);
    setDetail((current) => ({
      ...current,
      [id]: {
        ...(current[id] ?? EMPTY_DETAIL),
        loading: false,
        error: refreshed ? "" : "刷新失败",
      },
    }));
    load();
  };

  const loadTab = async (id: string, tab: DetailTab) => {
    const d = detail[id] ?? EMPTY_DETAIL;
    setDetail((current) => ({ ...current, [id]: { ...d, loading: true, error: "" } }));
    try {
      if (tab === "tools") {
        const tools = await listMcpRuntimeTools(id);
        setDetail((current) => ({ ...current, [id]: { ...(current[id] ?? EMPTY_DETAIL), tools, loading: false } }));
      } else if (tab === "resources") {
        const [resources, templates] = await Promise.all([listMcpResources(id), listMcpResourceTemplates(id)]);
        setDetail((current) => ({
          ...current,
          [id]: { ...(current[id] ?? EMPTY_DETAIL), resources, templates, loading: false },
        }));
      } else {
        const prompts = await listMcpPrompts(id);
        setDetail((current) => ({ ...current, [id]: { ...(current[id] ?? EMPTY_DETAIL), prompts, loading: false } }));
      }
    } catch {
      setDetail((current) => ({
        ...current,
        [id]: { ...(current[id] ?? EMPTY_DETAIL), loading: false, error: "加载失败" },
      }));
    }
  };

  const toggleDetail = (id: string) => {
    if (expandedId === id) {
      setExpandedId(null);
      return;
    }
    setExpandedId(id);
    setActiveTab("tools");
    if (!detail[id]) {
      setDetail((current) => ({ ...current, [id]: EMPTY_DETAIL }));
      loadTab(id, "tools");
    }
  };

  const switchTab = (id: string, tab: DetailTab) => {
    setActiveTab(tab);
    const d = detail[id] ?? EMPTY_DETAIL;
    const needsLoad =
      (tab === "tools" && d.tools === null) ||
      (tab === "resources" && (d.resources === null || d.templates === null)) ||
      (tab === "prompts" && d.prompts === null);
    if (needsLoad) loadTab(id, tab);
  };

  const openResource = async (id: string, resource: McpResourceDescriptor) => {
    setResourcePreviewState({ resource, result: null, loading: true, error: "" });
    const result = await readMcpResource(id, resource.uri);
    setResourcePreviewState((current) => current
      ? { ...current, result, loading: false, error: result ? "" : "读取失败" }
      : current);
  };

  const openPrompt = (id: string, prompt: McpPromptDescriptor) => {
    const formValues: Record<string, string> = {};
    prompt.arguments.forEach((arg) => { formValues[arg.name] = ""; });
    setPromptPreviewState({ serverId: id, prompt, formValues, result: null, loading: false, error: "" });
  };

  const submitPromptPreview = async () => {
    if (!promptPreviewState) return;
    setPromptPreviewState((current) => current ? { ...current, loading: true, error: "" } : current);
    const result = await getMcpPrompt(promptPreviewState.serverId, promptPreviewState.prompt.name, promptPreviewState.formValues);
    setPromptPreviewState((current) => current
      ? { ...current, result, loading: false, error: result ? "" : "获取失败" }
      : current);
  };

  const updatePromptFormValue = (name: string, value: string) => {
    setPromptPreviewState((current) => current
      ? { ...current, formValues: { ...current.formValues, [name]: value } }
      : current);
  };

  if (loading) {
    return (
      <div className="capability-loading-page page-canvas" aria-label="正在加载插件">
        <Skeleton className="h-20 w-full" />
        <div className="capability-skeleton-grid">
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-24 w-full" />
        </div>
      </div>
    );
  }

  if (error && !data) {
    return (
      <ErrorState
        title="插件暂时无法加载"
        description={error}
        action={<Button variant="secondary" onClick={() => void load()}>重试</Button>}
      />
    );
  }

  return (
    <div className="page-canvas flex h-full flex-col">
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
            <p className="text-sm font-medium">MCP 运行时（stdio + Streamable HTTP）</p>
            <p className="text-xs text-[var(--text-muted)] mt-0.5">
              每个 MCP 服务器按真实运行时状态展示；Tools / Resources / Prompts 可查看，但工具执行始终经过安全审批网关，前端不提供直接执行入口。
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
            {data.mcp.map((mcp) => {
              const isExpanded = expandedId === mcp.id;
              const d = detail[mcp.id] ?? EMPTY_DETAIL;
              return (
                <Panel key={mcp.id} className="group">
                  <div className="flex items-start justify-between">
                    <div className="flex items-center gap-3 min-w-0">
                      <Link className="w-5 h-5 text-[var(--accent)] flex-shrink-0" />
                      <div>
                        <div className="flex items-center gap-2 flex-wrap">
                          <h4 className="font-medium text-sm">{mcp.name}</h4>
                          {mcp.enabled ? (
                            <Badge tone="success">已启用</Badge>
                          ) : (
                            <Badge tone="default">已禁用</Badge>
                          )}
                          <CapabilityStatusBadge
                            tone={runtimeStatusTone(mcp.runtime_status || (mcp.enabled ? "disconnected" : "disabled"))}
                            label={runtimeStatusLabel(mcp.runtime_status || (mcp.enabled ? "disconnected" : "disabled"))}
                          />
                        </div>
                        <p className="text-xs text-[var(--text-muted)] mt-0.5 font-mono">
                          {mcpTransportRuntimeLabel(mcp.transport)}
                          {mcp.transport === "stdio" ? ` · ${mcp.command || "(未配置)"}` : ` · ${mcp.url || "(未配置)"}`}
                        </p>
                        <p className="text-[10px] text-[var(--text-faint)] mt-1">
                          {mcp.protocol_version ? `协议 ${mcp.protocol_version}` : "协议未协商"} · Tools {mcp.tools_count ?? 0} · Resources {mcp.resources_count ?? 0} · Prompts {mcp.prompts_count ?? 0}
                          {mcp.env && Object.keys(mcp.env).length > 0 ? ` · 环境变量 ${Object.keys(mcp.env).length} 个（值不展示）` : ""}
                        </p>
                        {mcp.safe_error && (
                          <p className="text-[10px] text-[var(--danger)] mt-0.5 truncate max-w-[420px]">
                            {mcp.safe_error}
                          </p>
                        )}
                        {testResults[mcp.id] && (
                          <p className={`text-xs mt-2 ${testResults[mcp.id].ok ? "text-[var(--success)]" : "text-[var(--danger)]"}`}>
                            {testResults[mcp.id].ok ? "连接成功" : "连接失败"}：{testResults[mcp.id].message}
                          </p>
                        )}
                      </div>
                    </div>

                    <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                      <button
                        onClick={() => handleRefresh(mcp.id)}
                        className="p-1.5 rounded hover:bg-[var(--panel-hover)] text-[var(--text-muted)] hover:text-[var(--text)]"
                        title="刷新运行时"
                        aria-label="刷新运行时"
                      >
                        <RefreshCw className="w-4 h-4" />
                      </button>
                      <button
                        onClick={() => toggleDetail(mcp.id)}
                        className="p-1.5 rounded hover:bg-[var(--panel-hover)] text-[var(--text-muted)] hover:text-[var(--text)]"
                        title="查看详情"
                        aria-label={isExpanded ? "收起详情" : "查看详情"}
                        aria-expanded={isExpanded}
                      >
                        {isExpanded ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
                      </button>
                      <button
                        onClick={() => handleToggle(mcp.id)}
                        className="p-1.5 rounded hover:bg-[var(--panel-hover)] text-[var(--text-muted)] hover:text-[var(--text)]"
                        title={mcp.enabled ? "禁用" : "启用"}
                        aria-label={mcp.enabled ? "禁用插件" : "启用插件"}
                        aria-pressed={mcp.enabled}
                      >
                        {mcp.enabled ? <ToggleRight className="w-4 h-4 text-[var(--success)]" /> : <ToggleLeft className="w-4 h-4" />}
                      </button>
                      <button
                        onClick={() => handleTest(mcp.id)}
                        disabled={testingId === mcp.id}
                        className="p-1.5 rounded hover:bg-[var(--panel-hover)] text-[var(--text-muted)] hover:text-[var(--text)]"
                        title="测试连接"
                        aria-label="测试连接"
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
                        aria-label="编辑插件"
                      >
                        <Wrench className="w-4 h-4" />
                      </button>
                      <button
                        onClick={() => handleDelete(mcp.id, mcp.name)}
                        className="p-1.5 rounded hover:bg-[var(--danger)]/20 text-[var(--text-muted)] hover:text-[var(--danger)]"
                        title="删除"
                        aria-label="删除插件"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </div>
                  </div>

                  {isExpanded && (
                    <McpDetailPanel
                      tab={activeTab}
                      detail={d}
                      onSwitchTab={(tab) => switchTab(mcp.id, tab)}
                      onOpenResource={(resource) => openResource(mcp.id, resource)}
                      onOpenPrompt={(prompt) => openPrompt(mcp.id, prompt)}
                    />
                  )}
                </Panel>
              );
            })}
          </div>
        )}

        <div className="flex items-center gap-3">
          <div className="flex-1 border-t border-[var(--border)]" />
          <span className="text-xs text-[var(--text-faint)] font-mono uppercase">Subagents</span>
          <div className="flex-1 border-t border-[var(--border)]" />
        </div>

        <Panel>
          <div className="flex items-start justify-between gap-3 mb-4">
            <div>
              <h3 className="text-sm font-semibold">Subagent Runtime</h3>
              <p className="text-xs text-[var(--text-muted)] mt-1">已发现的安全 metadata；不显示 private instructions 或内部路径。</p>
            </div>
            <Badge tone={subagents.some((agent) => agent.runtime_ready) ? "success" : "default"}>
              {subagents.some((agent) => agent.runtime_ready) ? "可用" : "未就绪"}
            </Badge>
          </div>

          {subagentError ? (
            <p className="text-sm text-[var(--danger)]">{subagentError}</p>
          ) : subagents.length === 0 ? (
            <p className="text-sm text-[var(--text-faint)]">当前未发现可用 Subagent。</p>
          ) : (
            <div className="space-y-3">
              {subagents.map((agent) => (
                <div key={agent.name} className="rounded-lg border border-[var(--border)] px-3 py-3">
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <h4 className="text-sm font-medium">{agent.name}</h4>
                      <p className="text-xs text-[var(--text-muted)] mt-1">{agent.description}</p>
                    </div>
                    <Badge tone={agent.runtime_ready ? "success" : "default"}>
                      {agent.runtime_ready ? "可用" : "未就绪"}
                    </Badge>
                  </div>
                  <p className="text-xs text-[var(--text-faint)] mt-2">
                    Model：{agent.model || "继承 Parent 配置"}
                  </p>
                  <div className="flex flex-wrap gap-1 mt-2">
                    {agent.allowed_tools.map((tool) => (
                      <Badge key={tool} tone="default">{tool}</Badge>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}

          <div className="mt-4 space-y-1 text-xs text-[var(--text-muted)]">
            <p>当前仅支持：Parent → Child 单层委派。</p>
            <p>暂不支持：嵌套审批、递归 Subagent、workdir override、AGENT.md hot reload。</p>
            <p>Subagent 调用属于高风险 Agent Delegation，Owner / Standard 需要审批，Restricted 拒绝。</p>
          </div>
        </Panel>
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
              onChange={(e) => setFormTransport(e.target.value as "stdio" | "streamable_http")}
              className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
            >
              <option value="stdio">Stdio (命令行)</option>
              <option value="streamable_http">Streamable HTTP</option>
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
              placeholder="例如：http://127.0.0.1:3001/mcp"
            />
          )}

          <div>
            <label className="block text-sm font-medium mb-1">环境变量 (JSON 对象；留空则保留现有值)</label>
            <p className="text-xs text-[var(--text-muted)] mb-1">环境变量值将保存到系统安全凭据库，不会写入项目数据库或回显。</p>
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
        </div>
      </Modal>

      {/* Resource Preview Modal */}
      <ResourcePreviewModal state={resourcePreviewState} onClose={() => setResourcePreviewState(null)} />

      {/* Prompt Preview Modal */}
      <PromptPreviewModal
        state={promptPreviewState}
        onClose={() => setPromptPreviewState(null)}
        onSubmit={submitPromptPreview}
        onFormChange={updatePromptFormValue}
      />
    </div>
  );
}

// ── Detail panel (Tools / Resources / Prompts) ──

function McpDetailPanel({
  tab,
  detail,
  onSwitchTab,
  onOpenResource,
  onOpenPrompt,
}: {
  tab: DetailTab;
  detail: DetailState;
  onSwitchTab: (tab: DetailTab) => void;
  onOpenResource: (resource: McpResourceDescriptor) => void;
  onOpenPrompt: (prompt: McpPromptDescriptor) => void;
}) {
  const tabs: { key: DetailTab; label: string; count: number }[] = [
    { key: "tools", label: "Tools", count: detail.tools?.length ?? 0 },
    { key: "resources", label: "Resources", count: (detail.resources?.length ?? 0) + (detail.templates?.length ?? 0) },
    { key: "prompts", label: "Prompts", count: detail.prompts?.length ?? 0 },
  ];

  return (
    <div className="mt-4 border-t border-[var(--border)] pt-3">
      <div className="flex items-center gap-2 mb-3">
        {tabs.map((t) => (
          <button
            key={t.key}
            onClick={() => onSwitchTab(t.key)}
            aria-pressed={tab === t.key}
            className={`px-3 py-1.5 text-xs rounded-md border transition-colors ${
              tab === t.key
                ? "border-[var(--accent)] bg-[var(--accent)]/10 text-[var(--accent)]"
                : "border-[var(--border)] text-[var(--text-muted)] hover:text-[var(--text)]"
            }`}
          >
            {t.label} ({t.count})
          </button>
        ))}
      </div>

      {detail.error && <p className="text-xs text-[var(--danger)] mb-2">{detail.error}</p>}
      {detail.loading ? (
        <div className="flex justify-center py-6"><Spinner className="w-5 h-5" /></div>
      ) : tab === "tools" ? (
        <ToolsTab tools={detail.tools} />
      ) : tab === "resources" ? (
        <ResourcesTab
          resources={detail.resources}
          templates={detail.templates}
          onOpenResource={onOpenResource}
        />
      ) : (
        <PromptsTab prompts={detail.prompts} onOpenPrompt={onOpenPrompt} />
      )}

      <p className="text-[10px] text-[var(--text-faint)] mt-3">
        {tab === "tools"
          ? "仅展示工具元数据；工具执行由安全审批网关统一处理，本页不提供执行按钮。"
          : tab === "resources"
            ? "资源为只读预览；二进制内容不会自动展开或下载。"
            : "Prompt 仅供预览，不会自动进入对话、任务或 System Prompt。"}
      </p>
    </div>
  );
}

function ToolsTab({ tools }: { tools: McpRuntimeTool[] | null }) {
  if (!tools || tools.length === 0) {
    return <p className="text-sm text-[var(--text-faint)]">无可用工具。</p>;
  }
  return (
    <div className="space-y-2">
      {tools.map((tool) => (
        <div key={tool.name} className="rounded-lg border border-[var(--border)] px-3 py-2">
          <div className="flex items-center gap-2">
            <code className="text-sm font-mono font-medium">{tool.name}</code>
            {tool.inputSchema && typeof tool.inputSchema === "object" && (
              <span className="text-[10px] text-[var(--text-faint)]">
                {Object.keys((tool.inputSchema as Record<string, unknown>).properties ?? {}).length} 个入参
              </span>
            )}
          </div>
          {tool.description && (
            <p className="text-xs text-[var(--text-muted)] mt-1">{tool.description}</p>
          )}
        </div>
      ))}
    </div>
  );
}

function ResourcesTab({
  resources,
  templates,
  onOpenResource,
}: {
  resources: McpResourceDescriptor[] | null;
  templates: McpResourceTemplate[] | null;
  onOpenResource: (resource: McpResourceDescriptor) => void;
}) {
  const hasResources = resources && resources.length > 0;
  const hasTemplates = templates && templates.length > 0;
  if (!hasResources && !hasTemplates) {
    return <p className="text-sm text-[var(--text-faint)]">无可用资源。</p>;
  }
  return (
    <div className="space-y-3">
      {hasResources && (
        <div>
          <h5 className="text-xs font-semibold text-[var(--text-muted)] mb-1">资源 ({resources!.length})</h5>
          <div className="space-y-1.5">
            {resources!.map((resource) => (
              <div key={resource.uri} className="flex items-center justify-between rounded-lg border border-[var(--border)] px-3 py-2">
                <div className="min-w-0">
                  <p className="text-sm font-medium truncate">{resource.name}</p>
                  <p className="text-[10px] text-[var(--text-faint)] font-mono truncate">{resource.uri}</p>
                  {resource.mimeType && <p className="text-[10px] text-[var(--text-faint)]">{resource.mimeType}</p>}
                </div>
                <Button size="sm" variant="secondary" onClick={() => onOpenResource(resource)}>
                  <Eye className="w-3.5 h-3.5" />
                  预览
                </Button>
              </div>
            ))}
          </div>
        </div>
      )}
      {hasTemplates && (
        <div>
          <h5 className="text-xs font-semibold text-[var(--text-muted)] mb-1">资源模板 ({templates!.length})</h5>
          <div className="space-y-1.5">
            {templates!.map((template) => (
              <div key={template.uriTemplate} className="rounded-lg border border-[var(--border)] px-3 py-2">
                <p className="text-sm font-medium">{template.name}</p>
                <p className="text-[10px] text-[var(--text-faint)] font-mono">{template.uriTemplate}</p>
                {template.mimeType && <p className="text-[10px] text-[var(--text-faint)]">{template.mimeType}</p>}
                {template.description && <p className="text-xs text-[var(--text-muted)] mt-1">{template.description}</p>}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function PromptsTab({
  prompts,
  onOpenPrompt,
}: {
  prompts: McpPromptDescriptor[] | null;
  onOpenPrompt: (prompt: McpPromptDescriptor) => void;
}) {
  if (!prompts || prompts.length === 0) {
    return <p className="text-sm text-[var(--text-faint)]">无可用 Prompt。</p>;
  }
  return (
    <div className="space-y-2">
      {prompts.map((prompt) => (
        <div key={prompt.name} className="flex items-center justify-between rounded-lg border border-[var(--border)] px-3 py-2">
          <div className="min-w-0">
            <p className="text-sm font-medium">{prompt.title || prompt.name}</p>
            {prompt.description && <p className="text-xs text-[var(--text-muted)] mt-1">{prompt.description}</p>}
            {prompt.arguments.length > 0 && (
              <p className="text-[10px] text-[var(--text-faint)] mt-1">
                参数：{prompt.arguments.map((a) => (a.required ? `${a.name}*` : a.name)).join(", ")}
              </p>
            )}
          </div>
          <Button size="sm" variant="secondary" onClick={() => onOpenPrompt(prompt)}>
            <Eye className="w-3.5 h-3.5" />
            预览
          </Button>
        </div>
      ))}
    </div>
  );
}

// ── Resource preview modal ──

function ResourcePreviewModal({
  state,
  onClose,
}: {
  state: {
    resource: McpResourceDescriptor;
    result: McpResourceReadResponse | null;
    loading: boolean;
    error: string;
  } | null;
  onClose: () => void;
}) {
  if (!state) return null;
  const { resource, result, loading, error } = state;

  return (
    <Modal open onClose={onClose} title={`资源预览 · ${resource.name}`}>
      <div className="space-y-3">
        <p className="text-[10px] text-[var(--text-faint)] font-mono break-all">{resource.uri}</p>
        {loading && (
          <div className="flex justify-center py-6"><Spinner className="w-5 h-5" /></div>
        )}
        {!loading && error && <p className="text-sm text-[var(--danger)]">{error}</p>}
        {!loading && result && "input_required" in result && (
          <div className="rounded-lg border border-[var(--border)] px-3 py-3">
            <p className="text-sm text-[var(--text-muted)]">该资源需要额外输入，当前预览不继续交互。</p>
          </div>
        )}
        {!loading && result && "contents" in result && (
          <div className="space-y-3">
            {result.contents.map((content, index) => {
              const preview = resourcePreview(content);
              if (preview.kind === "blob") {
                return (
                  <div key={index} className="rounded-lg border border-[var(--border)] px-3 py-3">
                    <p className="text-sm font-medium">Binary MCP Resource</p>
                    <p className="text-xs text-[var(--text-muted)] mt-1">MIME Type：{preview.mimeType || "未知"}</p>
                    <p className="text-xs text-[var(--text-muted)] mt-1">Encoded Size：{preview.size} 字符</p>
                    <p className="text-xs text-[var(--text-faint)] mt-2">内容未自动展开。</p>
                  </div>
                );
              }
              return (
                <pre
                  key={index}
                  className="whitespace-pre-wrap break-words rounded-lg border border-[var(--border)] bg-[var(--panel)]/50 px-3 py-3 text-xs max-h-64 overflow-y-auto scrollbar-thin"
                >
                  {preview.text}
                </pre>
              );
            })}
          </div>
        )}
      </div>
    </Modal>
  );
}

// ── Prompt preview modal ──

function PromptPreviewModal({
  state,
  onClose,
  onSubmit,
  onFormChange,
}: {
  state: {
    prompt: McpPromptDescriptor;
    formValues: Record<string, string>;
    result: McpPromptGetResponse | null;
    loading: boolean;
    error: string;
  } | null;
  onClose: () => void;
  onSubmit: () => void;
  onFormChange: (name: string, value: string) => void;
}) {
  if (!state) return null;
  const { prompt, formValues, result, loading, error } = state;

  return (
    <Modal open onClose={onClose} title={`Prompt 预览 · ${prompt.title || prompt.name}`}>
      <div className="space-y-3">
        <div className="rounded-lg border border-[var(--warning)]/30 bg-[var(--warning)]/10 px-3 py-2">
          <p className="text-xs text-[var(--warning)]">{EXTERNAL_PROMPT_WARNING}</p>
        </div>

        {prompt.arguments.length > 0 && (
          <div className="space-y-2">
            {prompt.arguments.map((arg) => (
              <div key={arg.name}>
                <label className="block text-xs font-medium text-[var(--text-muted)] mb-1">
                  {arg.name}{arg.required ? " *" : ""}
                </label>
                <input
                  value={formValues[arg.name] ?? ""}
                  onChange={(e) => onFormChange(arg.name, e.target.value)}
                  className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
                />
              </div>
            ))}
          </div>
        )}

        <div className="flex justify-end">
          <Button size="sm" onClick={onSubmit} disabled={loading}>
            {loading ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
            生成预览
          </Button>
        </div>

        {error && <p className="text-sm text-[var(--danger)]">{error}</p>}

        {result && "input_required" in result && (
          <div className="rounded-lg border border-[var(--border)] px-3 py-3">
            <p className="text-sm text-[var(--text-muted)]">该 Prompt 需要额外输入，当前预览不继续交互。</p>
          </div>
        )}

        {result && "messages" in result && (
          <div className="space-y-2">
            {result.messages.map((message, index) => (
              <div key={index} className="rounded-lg border border-[var(--border)] px-3 py-2">
                <p className="text-[10px] text-[var(--text-faint)] uppercase mb-1">{message.role}</p>
                <p className="text-xs whitespace-pre-wrap break-words">{promptMessageText(message)}</p>
              </div>
            ))}
          </div>
        )}
      </div>
    </Modal>
  );
}
