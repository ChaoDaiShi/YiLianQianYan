import { useCallback, useEffect, useMemo, useState } from "react";
import { Boxes, LayoutGrid, List, Plus, RefreshCw, Search } from "lucide-react";
import { Link, useNavigate } from "react-router-dom";
import {
  CapabilityDetailSection,
  CapabilityMetaRow,
  CapabilityStatusBadge,
  formatCapabilityKind,
  formatCapabilityProvider,
  formatCapabilityRisk,
  formatCapabilityStatus,
  formatPermission,
} from "../../components/capabilities";
import { CapabilityDescriptor, CapabilityKind, CapabilityProviderKind, CapabilityRefreshReport, CapabilityRuntimeStatus, getCapability, listCapabilities, refreshCapabilities } from "../../api/client";
import ManagementFrame from "./ManagementFrame";
import { Button, EmptyState, ErrorState, Input, Modal, Skeleton } from "../../components/ui";
import { getCapabilityManagementTarget } from "./capabilityManagement";
import { formatToolDisplayName } from "../../components/chat/toolDisplay";
import CapabilityAvailabilityView from "./CapabilityAvailabilityView";

const KIND_FILTERS: Array<CapabilityKind | "all"> = ["all", "tool", "mcp_tool", "mcp_resource", "mcp_resource_template", "mcp_prompt", "subagent", "agent", "workflow", "skill"];
const PROVIDER_FILTERS: Array<CapabilityProviderKind | "all"> = ["all", "builtin", "mcp", "subagent", "agent_runtime", "workflow_runtime", "skill_runtime"];
const STATUS_FILTERS: Array<CapabilityRuntimeStatus | "all"> = ["all", "ready", "unavailable", "disabled", "misconfigured", "degraded", "unknown"];

export function CapabilityRegistryContent({ initialKind = "all" }: { initialKind?: CapabilityKind | "all" }) {
  const navigate = useNavigate();
  const [capabilities, setCapabilities] = useState<CapabilityDescriptor[] | null>(null);
  const [kind, setKind] = useState<CapabilityKind | "all">(initialKind);
  const [provider, setProvider] = useState<CapabilityProviderKind | "all">("all");
  const [status, setStatus] = useState<CapabilityRuntimeStatus | "all">("all");
  const [search, setSearch] = useState("");
  const [viewMode, setViewMode] = useState<"list" | "card">("list");
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
      if (!result.ok) {
        setError(result.error);
        return;
      }
      setReport(result.data);
      await reload();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setRefreshing(false);
    }
  };

  const handleListKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    if (!visibleCapabilities.length) return;
    e.preventDefault();
    const currentIndex = visibleCapabilities.findIndex((c) => c.id === selectedId);
    let nextIndex = currentIndex;
    if (e.key === "ArrowDown") {
      nextIndex = Math.min(currentIndex + 1, visibleCapabilities.length - 1);
    } else if (e.key === "ArrowUp") {
      nextIndex = Math.max(currentIndex - 1, 0);
    }
    if (nextIndex !== currentIndex && visibleCapabilities[nextIndex]) {
      const next = visibleCapabilities[nextIndex];
      document.getElementById(`capability-option-${next.id}`)?.focus();
      void selectCapability(next);
    }
  };

  return (
    <ManagementFrame embedded variant="workspace"
      title="能力"
      description="了解忆涟能做什么，查看可用状态与所需权限。"
      className="capability-page page-canvas ui-opt-capabilities"
      actions={
        <>
          <Button variant="secondary" onClick={() => setSourceChooserOpen(true)}>
            <Plus className="h-4 w-4" />
            新增能力来源
          </Button>
          <Button
            variant="secondary"
            onClick={() => void handleRefresh()}
            disabled={refreshing}
            loading={refreshing}
            loadingLabel="刷新中…"
          >
            <RefreshCw className="h-4 w-4" />
            刷新能力
          </Button>
        </>
      }
    >
      <div className="capability-page-body capability-page-body-split capability-catalog">
        {report && (
          <div className="capability-refresh-report" role="status">
            发现 {report.discovered} · 就绪 {report.ready} · 不可用 {report.unavailable} · 重复 {report.duplicates} · Provider 失败 {report.provider_failures}
          </div>
        )}
        {error && (
          <ErrorState
            title="能力信息暂时无法加载"
            description={error}
            action={
              <Button variant="secondary" size="sm" onClick={() => void reload()}>
                重试
              </Button>
            }
          />
        )}

        <section className="capability-filter-surface" aria-label="能力筛选">
          <div className="capability-search-field">
            <Search className="h-4 w-4 shrink-0 text-[var(--text-faint)]" />
            <Input
              aria-label="搜索能力"
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="搜索名称、描述或类型…"
            />
          </div>
          <FilterSelect
            label="能力类型"
            value={kind}
            onChange={(value) => setKind(value as CapabilityKind | "all")}
            options={KIND_FILTERS.map((value) => ({
              value,
              label: value === "all" ? "全部类型" : formatCapabilityKind(value),
            }))}
          />
          <FilterSelect
            label="来源"
            value={provider}
            onChange={(value) => setProvider(value as CapabilityProviderKind | "all")}
            options={PROVIDER_FILTERS.map((value) => ({
              value,
              label: value === "all" ? "全部来源" : formatCapabilityProvider(value),
            }))}
          />
          <FilterSelect
            label="状态"
            value={status}
            onChange={(value) => setStatus(value as CapabilityRuntimeStatus | "all")}
            options={STATUS_FILTERS.map((value) => ({
              value,
              label: value === "all" ? "全部状态" : formatCapabilityStatus(value).label,
            }))}
          />
        </section>

        <div className="capability-split-layout">
          <section
            className="capability-list-panel focus:outline-none"
            aria-label="能力列表"
            tabIndex={0}
            onKeyDown={handleListKeyDown}
          >
            <div className="capability-list-heading">
              <p>{visibleCapabilities.length} 项能力</p>
              <div className="flex items-center gap-1 bg-[var(--surface-muted)] p-0.5 rounded-md border border-[var(--border-soft)]">
                <button
                  type="button"
                  onClick={() => setViewMode("list")}
                  aria-label="列表视图"
                  aria-pressed={viewMode === "list"}
                  className={`p-1.5 rounded transition-colors ${
                    viewMode === "list"
                      ? "bg-[var(--surface-solid)] text-[var(--text)] shadow-xs"
                      : "text-[var(--text-muted)] hover:text-[var(--text)]"
                  }`}
                >
                  <List className="h-3.5 w-3.5" />
                </button>
                <button
                  type="button"
                  onClick={() => setViewMode("card")}
                  aria-label="卡片视图"
                  aria-pressed={viewMode === "card"}
                  className={`p-1.5 rounded transition-colors ${
                    viewMode === "card"
                      ? "bg-[var(--surface-solid)] text-[var(--text)] shadow-xs"
                      : "text-[var(--text-muted)] hover:text-[var(--text)]"
                  }`}
                >
                  <LayoutGrid className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>

            <div className="capability-list-scroll scrollbar-thin">
              {capabilities === null ? (
                <div className="capability-skeleton-stack" aria-label="正在加载能力">
                  <Skeleton className="h-20 w-full" />
                  <Skeleton className="h-20 w-full" />
                  <Skeleton className="h-20 w-full" />
                  <Skeleton className="h-20 w-full" />
                </div>
              ) : visibleCapabilities.length === 0 ? (
                <EmptyState
                  icon={<Boxes className="h-6 w-6" />}
                  title="暂时没有可显示的能力信息。"
                  description="没有匹配当前筛选条件的能力。"
                  className="py-16"
                />
              ) : (
                <div
                  className={`capability-catalog-results capability-catalog-results--${viewMode}`}
                  role="listbox"
                  aria-label="能力列表"
                  aria-activedescendant={selectedId ? `capability-option-${selectedId}` : undefined}
                >
                  {visibleCapabilities.map((capability) => {
                    const presentation = formatCapabilityStatus(capability.status);
                    const isSelected = capability.id === selectedId;
                    return (
                      <button
                        type="button"
                        key={capability.id}
                        id={`capability-option-${capability.id}`}
                        role="option"
                        tabIndex={isSelected ? 0 : -1}
                        aria-selected={isSelected}
                        onClick={() => void selectCapability(capability)}
                        className="capability-catalog-row"
                      >
                        <span className="capability-catalog-row-title">
                          <strong>{capability.kind === "tool" ? formatToolDisplayName(capability.name) : capability.name}</strong>
                          <CapabilityStatusBadge
                            label={presentation.label}
                            tone={presentation.tone}
                          />
                        </span>
                        {viewMode === "card" && capability.description && <span className="capability-catalog-row-description">{capability.description}</span>}
                        <span className="capability-catalog-row-meta">
                          {formatCapabilityKind(capability.kind)} · {formatCapabilityProvider(capability.provider)}
                        </span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          </section>

          <section className="capability-detail-panel" aria-label="能力详情">
            {selectedDetail ? (
              <CapabilityDetail
                capability={selectedDetail}
                loading={detailLoading}
                error={detailError}
                onRetry={() => {
                  if (selectedDetail) void selectCapability(selectedDetail);
                }}
              />
            ) : (
              <EmptyState
                icon={<Boxes className="h-6 w-6" />}
                title="选择一项能力"
                description="查看用途、权限与执行环境。"
              />
            )}
          </section>
        </div>

      </div>

      <Modal open={sourceChooserOpen} onClose={() => setSourceChooserOpen(false)} title="新增能力来源">
        <div className="space-y-3">
          <Link className="capability-owner-link" to="/capabilities?category=import">导入与本地发现</Link>
          <SourceChoice
            title="新建技能"
            description="编写一个工作区 SKILL.md。"
            onClick={() => navigate("/capabilities?category=skill")}
          />
          <SourceChoice
            title="添加 MCP 服务"
            description="连接提供工具的本地或远程 MCP 服务。"
            onClick={() => navigate("/capabilities?category=mcp")}
          />
          <SourceChoice
            title="管理智能体"
            description="创建或调整可被调用的智能体。"
            onClick={() => navigate("/agents")}
          />
          <SourceChoice
            title="管理工作流"
            description="创建或调整可执行工作流。"
            onClick={() => navigate("/capabilities?category=workflow")}
          />
        </div>
      </Modal>
    </ManagementFrame>
  );
}

function SourceChoice({ title, description, onClick }: { title: string; description: string; onClick: () => void }) {
  return (
    <Button
      variant="secondary"
      className="h-auto w-full justify-start px-4 py-3 text-left"
      onClick={onClick}
    >
      <span>
        <strong className="block text-sm">{title}</strong>
        <span className="mt-1 block text-xs font-normal text-[var(--text-secondary)]">
          {description}
        </span>
      </span>
    </Button>
  );
}

function FilterSelect({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: Array<{ value: string; label: string }>;
}) {
  return (
    <label className="capability-filter-select">
      <span>{label}</span>
      <select aria-label={label} value={value} onChange={(event) => onChange(event.target.value)}>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}

function CapabilityDetail({
  capability,
  loading,
  error,
  onRetry,
}: {
  capability: CapabilityDescriptor;
  loading: boolean;
  error: string;
  onRetry?: () => void;
}) {
  const status = formatCapabilityStatus(capability.status);
  const risk = formatCapabilityRisk(capability.risk);
  const managementTarget = getCapabilityManagementTarget(capability);
  const provider = formatCapabilityProvider(capability.provider);

  return (
    <div className="capability-detail-content">
      <div className="capability-detail-heading capability-catalog-detail-heading">
        <div>
          <div className="capability-detail-title-line">
            <Boxes className="h-5 w-5 text-[var(--accent-primary)]" />
            <h2>{capability.kind === "tool" ? formatToolDisplayName(capability.name) : capability.name}</h2>
            <CapabilityStatusBadge label={status.label} tone={status.tone} />
          </div>
          <p className="capability-detail-description">{capability.description || "暂无描述"}</p>
        </div>
        {managementTarget && (
          <Link className="capability-owner-link" to={managementTarget.to}>{managementTarget.label}</Link>
        )}
      </div>

      {loading && <div className="capability-detail-loading"><Skeleton className="h-4 w-1/2" /></div>}
      {error && (
        <div className="space-y-2">
          <ErrorState title="能力详情暂时无法加载" description={error} />
          {onRetry && <Button variant="secondary" size="sm" onClick={onRetry}>重试加载详情</Button>}
        </div>
      )}

      <CapabilityDetailSection title="能力概览">
        <dl className="capability-meta-list">
          <CapabilityMetaRow label="类型" value={formatCapabilityKind(capability.kind)} />
          <CapabilityMetaRow label="来源" value={provider} />
          {capability.metadata.source_name && capability.metadata.source_name !== capability.name && capability.metadata.source_name !== provider && (
            <CapabilityMetaRow label="来源名称" value={capability.metadata.source_name} />
          )}
          <CapabilityMetaRow label="风险" value={risk.label} />
          <CapabilityMetaRow label="版本" value={capability.metadata.version} />
          {capability.status !== "disabled" && <CapabilityMetaRow label="启用状态" value={capability.enabled ? "已启用" : "已禁用"} />}
        </dl>
      </CapabilityDetailSection>

      <CapabilityDetailSection title="权限与作用域">
        <div className="capability-permission-list">
          {capability.permissions.length > 0 ? (
            capability.permissions.map((p) => (
              <div key={p.permission}>
                <span>{formatPermission(p.permission)}</span>
                <code>{p.permission}</code>
                <em>{p.required ? "必需" : "可选"}</em>
              </div>
            ))
          ) : (
            <span>当前没有返回权限信息。</span>
          )}
        </div>
      </CapabilityDetailSection>

      <CapabilityAvailabilityView capability={capability} />

      <CapabilityDetailSection title="技术详情" technical>
        <dl className="capability-meta-list">
          <CapabilityMetaRow label="原始名称" value={capability.name} />
          <CapabilityMetaRow label="能力 ID" value={capability.id} mono />
          <CapabilityMetaRow label="原始类型" value={capability.kind} mono />
          <CapabilityMetaRow label="原始 Provider" value={capability.provider} mono />
          <CapabilityMetaRow label="来源 ID" value={capability.metadata.source_id} mono />
          <CapabilityMetaRow label="运行时就绪" value={capability.metadata.runtime_ready ? "是" : "否"} />
        </dl>
        {capability.input_schema && (
          <pre className="capability-content-preview mt-3">{JSON.stringify(capability.input_schema, null, 2)}</pre>
        )}
      </CapabilityDetailSection>
    </div>
  );
}


