import { useCallback, useEffect, useMemo, useState } from "react";
import { Boxes, Plus, RefreshCw, Search, Settings2 } from "lucide-react";
import { useNavigate } from "react-router-dom";
import {
  CapabilityDetailSection,
  CapabilityMetaRow,
  CapabilityStatusBadge,
  formatCapabilityKind,
  formatCapabilityProvider,
  formatCapabilityRisk,
  formatCapabilityStatus,
  formatPermission,
} from "../components/capabilities";
import {
  CapabilityDescriptor,
  CapabilityKind,
  CapabilityProviderKind,
  CapabilityRefreshReport,
  CapabilityRuntimeStatus,
  getCapability,
  listCapabilities,
  refreshCapabilities,
} from "../api/client";
import { Button, EmptyState, ErrorState, Input, Modal, PageHeader, Skeleton } from "../components/ui";
import { getCapabilityManagementTarget } from "../features/capabilities/capabilityManagement";
import ManagedImports from "../features/capabilities/ManagedImports";

const KIND_FILTERS: Array<CapabilityKind | "all"> = ["all", "tool", "mcp_tool", "subagent", "agent", "workflow", "skill"];
const PROVIDER_FILTERS: Array<CapabilityProviderKind | "all"> = ["all", "builtin", "mcp", "subagent", "agent_runtime", "workflow_runtime", "skill_runtime"];
const STATUS_FILTERS: Array<CapabilityRuntimeStatus | "all"> = ["all", "ready", "unavailable", "disabled", "misconfigured", "degraded", "unknown"];

export default function CapabilitiesPage() {
  const navigate = useNavigate();
  const [capabilities, setCapabilities] = useState<CapabilityDescriptor[] | null>(null);
  const [kind, setKind] = useState<CapabilityKind | "all">("all");
  const [provider, setProvider] = useState<CapabilityProviderKind | "all">("all");
  const [status, setStatus] = useState<CapabilityRuntimeStatus | "all">("all");
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedDetail, setSelectedDetail] = useState<CapabilityDescriptor | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState("");
  const [error, setError] = useState("");
  const [report, setReport] = useState<CapabilityRefreshReport | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [sourceChooserOpen, setSourceChooserOpen] = useState(false);

  const reload = useCallback(async () => {
    setError("");
    try {
      const result = await listCapabilities({
        q: search.trim() || undefined,
        kind: kind === "all" ? undefined : kind,
        provider: provider === "all" ? undefined : provider,
        status: status === "all" ? undefined : status,
        limit: 200,
      });
      if (result.ok) setCapabilities(result.data.capabilities);
      else setError(result.error);
    } catch (cause) {
      setError(String(cause));
    }
  }, [kind, provider, search, status]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const visibleCapabilities = useMemo(() => capabilities ?? [], [capabilities]);

  useEffect(() => {
    if (!selectedId || !visibleCapabilities.some((capability) => capability.id === selectedId)) {
      setSelectedId(visibleCapabilities[0]?.id ?? null);
      setSelectedDetail(visibleCapabilities[0] ?? null);
    }
  }, [selectedId, visibleCapabilities]);

  const selectCapability = async (capability: CapabilityDescriptor) => {
    setSelectedId(capability.id);
    setSelectedDetail(capability);
    setDetailError("");
    setDetailLoading(true);
    try {
      const result = await getCapability(capability.id);
      if (result.ok) setSelectedDetail(result.data);
      else setDetailError(result.error);
    } catch (cause) {
      setDetailError(String(cause));
    } finally {
      setDetailLoading(false);
    }
  };

  const handleRefresh = async () => {
    setRefreshing(true);
    setError("");
    try {
      const result = await refreshCapabilities();
      if (result.ok) setReport(result.data);
      else setError(result.error);
      await reload();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setRefreshing(false);
    }
  };

  return (
    <div className="capability-page page-canvas">
      <PageHeader
        title="能力"
        description="查看系统能力，并前往真实来源完成新增、编辑或删除。"
        actions={<><Button variant="secondary" onClick={() => setSourceChooserOpen(true)}><Plus className="h-4 w-4" />新增能力来源</Button><Button onClick={() => void handleRefresh()} disabled={refreshing}><RefreshCw className={`h-4 w-4 ${refreshing ? "animate-spin" : ""}`} />刷新能力</Button></>}
      />

      <div className="capability-page-body">
        <div className="mb-3 flex flex-wrap gap-2" aria-label="能力分类">
          <Button variant="secondary" size="sm" onClick={() => { setKind("skill"); setProvider("all"); }}>Skill</Button>
          <Button variant="secondary" size="sm" onClick={() => { setKind("mcp_tool"); setProvider("mcp"); }}>MCP</Button>
          <a href="#managed-imports" className="rounded-lg border border-[var(--border-soft)] px-3 py-2 text-xs">声明式 Plugin</a>
          <Button variant="secondary" size="sm" onClick={() => { setKind("agent"); setProvider("all"); }}>Agent</Button>
          <Button variant="secondary" size="sm" onClick={() => { setKind("workflow"); setProvider("all"); }}>Workflow</Button>
          <Button variant="secondary" size="sm" onClick={() => { setKind("tool"); setProvider("builtin"); }}>内置能力</Button>
        </div>
        <ManagedImports />
        {report && <div className="capability-refresh-report" role="status">发现 {report.discovered} · 就绪 {report.ready} · 不可用 {report.unavailable} · 重复 {report.duplicates} · Provider 失败 {report.provider_failures}</div>}
        {error && <ErrorState title="能力信息暂时无法加载" description={error} action={<Button variant="secondary" size="sm" onClick={() => void reload()}>重试</Button>} />}

        <section className="capability-filter-surface" aria-label="能力筛选">
          <div className="capability-search-field">
            <Search className="h-4 w-4 shrink-0 text-[var(--text-faint)]" />
            <Input aria-label="搜索能力" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="搜索名称、描述或类型…" />
          </div>
          <FilterSelect label="能力类型" value={kind} onChange={(value) => setKind(value as CapabilityKind | "all")} options={KIND_FILTERS.map((value) => ({ value, label: value === "all" ? "全部类型" : formatCapabilityKind(value) }))} />
          <FilterSelect label="Provider" value={provider} onChange={(value) => setProvider(value as CapabilityProviderKind | "all")} options={PROVIDER_FILTERS.map((value) => ({ value, label: value === "all" ? "全部来源" : formatCapabilityProvider(value) }))} />
          <FilterSelect label="状态" value={status} onChange={(value) => setStatus(value as CapabilityRuntimeStatus | "all")} options={STATUS_FILTERS.map((value) => ({ value, label: value === "all" ? "全部状态" : formatCapabilityStatus(value).label }))} />
        </section>

        <div className="capability-split-layout">
          <section className="capability-list-panel" aria-label="能力列表">
            <div className="capability-list-heading"><div><h2>系统能力</h2><p>{visibleCapabilities.length} 项真实记录</p></div></div>
            <div className="capability-list-scroll scrollbar-thin">
              {capabilities === null ? (
                <div className="capability-skeleton-stack" aria-label="正在加载能力"><Skeleton className="h-20 w-full" /><Skeleton className="h-20 w-full" /><Skeleton className="h-20 w-full" /><Skeleton className="h-20 w-full" /></div>
              ) : visibleCapabilities.length === 0 ? (
                <EmptyState icon={<Boxes className="h-6 w-6" />} title="暂时没有可显示的能力信息。" description="没有匹配当前筛选条件的能力。" className="py-16" />
              ) : (
                <div className="capability-row-stack">
                  {visibleCapabilities.map((capability) => {
                    const presentation = formatCapabilityStatus(capability.status);
                    return (
                      <button type="button" key={capability.id} aria-selected={capability.id === selectedId} onClick={() => void selectCapability(capability)} className={`capability-list-row ${capability.id === selectedId ? "capability-list-row-selected" : ""}`}>
                        <span className="capability-list-row-heading"><Boxes className="h-4 w-4 shrink-0 text-[var(--accent-purple)]" /><strong>{capability.name}</strong><CapabilityStatusBadge label={presentation.label} tone={presentation.tone} /></span>
                        <span className="capability-list-row-description">{capability.description || "暂无描述"}</span>
                        <span className="capability-list-row-meta">{formatCapabilityKind(capability.kind)} · {formatCapabilityProvider(capability.provider)}</span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          </section>

          <section className="capability-detail-panel" aria-label="能力详情">
            {selectedDetail ? <CapabilityDetail capability={selectedDetail} loading={detailLoading} error={detailError} onManage={(to) => navigate(to)} /> : <EmptyState icon={<Boxes className="h-6 w-6" />} title="选择一项能力" description="查看真实的能力、权限与状态字段。" className="h-full min-h-[320px]" />}
          </section>
        </div>
      </div>

      <Modal open={sourceChooserOpen} onClose={() => setSourceChooserOpen(false)} title="新增能力来源">
        <div className="space-y-3">
          <p className="text-sm text-[var(--text-secondary)]">能力注册表会自动发现下面这些真实来源，不会创建无法执行的空能力。</p>
          <SourceChoice title="新建技能" description="编写一个工作区 SKILL.md。" onClick={() => navigate("/skills")} />
          <SourceChoice title="添加 MCP 服务" description="连接提供工具的本地或远程 MCP 服务。" onClick={() => navigate("/plugins")} />
          <SourceChoice title="管理智能体" description="创建或调整可被调用的智能体。" onClick={() => navigate("/agents")} />
          <SourceChoice title="管理工作流" description="创建或调整可执行工作流。" onClick={() => navigate("/workflows")} />
        </div>
      </Modal>
    </div>
  );
}

function SourceChoice({ title, description, onClick }: { title: string; description: string; onClick: () => void }) {
  return <Button variant="secondary" className="h-auto w-full justify-start px-4 py-3 text-left" onClick={onClick}><span><strong className="block text-sm">{title}</strong><span className="mt-1 block text-xs font-normal text-[var(--text-secondary)]">{description}</span></span></Button>;
}

function FilterSelect({ label, value, onChange, options }: { label: string; value: string; onChange: (value: string) => void; options: Array<{ value: string; label: string }> }) {
  return (
    <label className="capability-filter-select"><span>{label}</span><select aria-label={label} value={value} onChange={(event) => onChange(event.target.value)}>{options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select></label>
  );
}

function CapabilityDetail({ capability, loading, error, onManage }: { capability: CapabilityDescriptor; loading: boolean; error: string; onManage: (to: string) => void }) {
  const status = formatCapabilityStatus(capability.status);
  const risk = formatCapabilityRisk(capability.risk);
  const managementTarget = getCapabilityManagementTarget(capability);
  return (
    <div className="capability-detail-content">
      <div className="capability-detail-heading"><div><div className="capability-detail-title-line"><Boxes className="h-5 w-5 text-[var(--accent-primary)]" /><h2>{capability.name}</h2><CapabilityStatusBadge label={status.label} tone={status.tone} /></div><p className="capability-detail-description">{capability.description || "暂无描述"}</p></div>{managementTarget && <Button variant="secondary" size="sm" onClick={() => onManage(managementTarget.to)}><Settings2 className="h-4 w-4" />管理来源</Button>}</div>
      {loading && <div className="capability-detail-loading"><Skeleton className="h-4 w-1/2" /></div>}
      {error && <ErrorState title="能力详情暂时无法加载" description={error} />}
      <CapabilityDetailSection title="能力概览">
        <dl className="capability-meta-list"><CapabilityMetaRow label="类型" value={formatCapabilityKind(capability.kind)} /><CapabilityMetaRow label="Provider" value={formatCapabilityProvider(capability.provider)} /><CapabilityMetaRow label="风险" value={risk.label} /><CapabilityMetaRow label="启用状态" value={capability.enabled ? "已启用" : "已禁用"} /></dl>
        {!managementTarget && <p className="mt-3 text-sm text-[var(--text-secondary)]">这是由系统运行时提供的内置能力，不能在注册表中直接编辑或删除。</p>}
      </CapabilityDetailSection>
      <CapabilityDetailSection title="权限与作用域">
        <div className="capability-permission-list">{capability.permissions.length > 0 ? capability.permissions.map((permission) => <div key={permission.permission}><span>{formatPermission(permission.permission)}</span><code>{permission.permission}</code><em>{permission.required ? "必需" : "可选"}</em></div>) : <span>当前没有返回权限信息。</span>}</div>
      </CapabilityDetailSection>
      <CapabilityDetailSection title="技术详情" technical>
        <dl className="capability-meta-list"><CapabilityMetaRow label="能力 ID" value={capability.id} mono /><CapabilityMetaRow label="原始类型" value={capability.kind} mono /><CapabilityMetaRow label="原始 Provider" value={capability.provider} mono /><CapabilityMetaRow label="来源 ID" value={capability.metadata.source_id} mono /><CapabilityMetaRow label="来源名称" value={capability.metadata.source_name} /><CapabilityMetaRow label="版本" value={capability.metadata.version} mono /><CapabilityMetaRow label="运行时就绪" value={capability.metadata.runtime_ready ? "是" : "否"} /></dl>{capability.input_schema && <pre className="capability-content-preview mt-3">{JSON.stringify(capability.input_schema, null, 2)}</pre>}
      </CapabilityDetailSection>
    </div>
  );
}
