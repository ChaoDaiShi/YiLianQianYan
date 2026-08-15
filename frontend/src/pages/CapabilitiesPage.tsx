import { useCallback, useEffect, useState } from "react";
import { Boxes, RefreshCw } from "lucide-react";
import {
  Badge,
  Button,
  PageHeader,
  Panel,
  EmptyState,
  Spinner,
} from "../components/ui";
import {
  CapabilityDescriptor,
  CapabilityKind,
  CapabilityRefreshReport,
  listCapabilities,
  refreshCapabilities,
} from "../api/client";

const KIND_LABELS: Record<CapabilityKind, string> = {
  tool: "工具",
  mcp_tool: "MCP 工具",
  subagent: "子智能体",
  agent: "Agent",
  workflow: "工作流",
  skill: "技能",
};

const STATUS_TONE: Record<string, "default" | "success" | "warning" | "danger" | "info"> = {
  ready: "success",
  unavailable: "danger",
  disabled: "default",
  misconfigured: "warning",
  degraded: "warning",
  unknown: "default",
};

const STATUS_LABEL: Record<string, string> = {
  ready: "就绪",
  unavailable: "不可用",
  disabled: "禁用",
  misconfigured: "配置错误",
  degraded: "降级",
  unknown: "未知",
};

const RISK_TONE: Record<string, "default" | "success" | "warning" | "danger" | "info"> = {
  low: "success",
  medium: "warning",
  high: "danger",
  dynamic: "info",
};

const KIND_FILTERS: Array<CapabilityKind | "all"> = [
  "all",
  "tool",
  "mcp_tool",
  "subagent",
  "agent",
  "workflow",
  "skill",
];

export default function CapabilitiesPage() {
  const [capabilities, setCapabilities] = useState<CapabilityDescriptor[] | null>(null);
  const [kind, setKind] = useState<CapabilityKind | "all">("all");
  const [report, setReport] = useState<CapabilityRefreshReport | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  const reload = useCallback(async () => {
    const res = await listCapabilities({ kind: kind === "all" ? undefined : kind, limit: 200 });
    if (res.ok) setCapabilities(res.data.capabilities);
  }, [kind]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const handleRefresh = async () => {
    setRefreshing(true);
    const res = await refreshCapabilities();
    if (res.ok) setReport(res.data);
    setRefreshing(false);
    void reload();
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="能力"
        description="统一能力注册表：发现系统的工具、MCP、子智能体、Agent、工作流与技能。此页面只做发现，不执行任何能力。"
        actions={
          <Button onClick={handleRefresh} disabled={refreshing}>
            <RefreshCw className={`h-4 w-4 ${refreshing ? "animate-spin" : ""}`} />
            刷新能力
          </Button>
        }
      />

      {report && (
        <div className="mx-4 mt-3 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--panel)] px-4 py-2 text-sm text-[var(--text-muted)]">
          发现 {report.discovered} · 就绪 {report.ready} · 不可用 {report.unavailable} · 重复{" "}
          {report.duplicates} · Provider 失败 {report.provider_failures}
        </div>
      )}

      <div className="mx-4 mt-3 flex flex-wrap gap-2">
        {KIND_FILTERS.map((k) => (
          <Button
            key={k}
            size="sm"
            variant={kind === k ? "primary" : "ghost"}
            onClick={() => setKind(k)}
          >
            {k === "all" ? "全部" : KIND_LABELS[k]}
          </Button>
        ))}
      </div>

      <div className="mx-4 mt-3 flex-1 overflow-y-auto pb-4">
        {capabilities === null ? (
          <div className="flex justify-center py-12">
            <Spinner />
          </div>
        ) : capabilities.length === 0 ? (
          <EmptyState
            icon={<Boxes className="h-6 w-6" />}
            title="暂无能力"
            description="没有匹配的能力。"
          />
        ) : (
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {capabilities.map((capability) => (
              <Panel key={capability.id}>
                <div className="flex items-start justify-between gap-2">
                  <div>
                    <h4 className="font-medium text-[var(--text)]">{capability.name}</h4>
                    <p className="text-xs text-[var(--text-faint)]">{capability.id}</p>
                  </div>
                  <Badge tone={STATUS_TONE[capability.status] ?? "default"}>
                    {STATUS_LABEL[capability.status] ?? capability.status}
                  </Badge>
                </div>
                {capability.description && (
                  <p className="mt-1 line-clamp-2 text-sm text-[var(--text-muted)]">
                    {capability.description}
                  </p>
                )}
                <div className="mt-2 flex items-center gap-2">
                  <Badge tone="default">{KIND_LABELS[capability.kind]}</Badge>
                  <Badge tone={RISK_TONE[capability.risk] ?? "default"}>
                    风险：{capability.risk}
                  </Badge>
                </div>
              </Panel>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
