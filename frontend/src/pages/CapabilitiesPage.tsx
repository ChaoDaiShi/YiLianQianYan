import { useCallback, useEffect, useMemo, useState } from "react";
import { Boxes, RefreshCw, Search } from "lucide-react";
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
import { Button, EmptyState, ErrorState, Input, PageHeader, Skeleton } from "../components/ui";

const KIND_FILTERS: Array<CapabilityKind | "all"> = ["all", "tool", "mcp_tool", "subagent", "agent", "workflow", "skill"];
const PROVIDER_FILTERS: Array<CapabilityProviderKind | "all"> = ["all", "builtin", "mcp", "subagent", "agent_runtime", "workflow_runtime", "skill_runtime"];
const STATUS_FILTERS: Array<CapabilityRuntimeStatus | "all"> = ["all", "ready", "unavailable", "disabled", "misconfigured", "degraded", "unknown"];

export default function CapabilitiesPage() {
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
        description="查看当前系统可以调用的基础能力。此页面只做发现，不执行任何能力。"
        actions={<Button onClick={() => void handleRefresh()} disabled={refreshing}><RefreshCw className={`h-4 w-4 ${refreshing ? "animate-spin" : ""}`} />刷新能力</Button>}
      />

      <div className="capability-page-body">
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
            {selectedDetail ? <CapabilityDetail capability={selectedDetail} loading={detailLoading} error={detailError} /> : <EmptyState icon={<Boxes className="h-6 w-6" />} title="选择一项能力" description="查看真实的能力、权限与状态字段。" className="h-full min-h-[320px]" />}
          </section>
        </div>
      </div>
    </div>
  );
}

function FilterSelect({ label, value, onChange, options }: { label: string; value: string; onChange: (value: string) => void; options: Array<{ value: string; label: string }> }) {
  return (
    <label className="capability-filter-select"><span>{label}</span><select aria-label={label} value={value} onChange={(event) => onChange(event.target.value)}>{options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select></label>
  );
}

function CapabilityDetail({ capability, loading, error }: { capability: CapabilityDescriptor; loading: boolean; error: string }) {
  const status = formatCapabilityStatus(capability.status);
  const risk = formatCapabilityRisk(capability.risk);
  return (
    <div className="capability-detail-content">
      <div className="capability-detail-heading"><div><div className="capability-detail-title-line"><Boxes className="h-5 w-5 text-[var(--accent-primary)]" /><h2>{capability.name}</h2><CapabilityStatusBadge label={status.label} tone={status.tone} /></div><p className="capability-detail-description">{capability.description || "暂无描述"}</p></div></div>
      {loading && <div className="capability-detail-loading"><Skeleton className="h-4 w-1/2" /></div>}
      {error && <ErrorState title="能力详情暂时无法加载" description={error} />}
      <CapabilityDetailSection title="能力概览">
        <dl className="capability-meta-list"><CapabilityMetaRow label="类型" value={formatCapabilityKind(capability.kind)} /><CapabilityMetaRow label="Provider" value={formatCapabilityProvider(capability.provider)} /><CapabilityMetaRow label="风险" value={risk.label} /><CapabilityMetaRow label="启用状态" value={capability.enabled ? "已启用" : "已禁用"} /></dl>
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
