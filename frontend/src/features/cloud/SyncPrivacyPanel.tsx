import React, { useEffect, useState } from "react";
import {
  Shield,
  Lock,
  CheckCircle2,
  AlertTriangle,
  RefreshCw,
  EyeOff,
  Database,
  Sliders,
  Info,
} from "lucide-react";
import { Badge, Button, Panel, Spinner } from "../../components/ui";
import {
  getPrivacySummary,
  getCloudIdentity,
  getCloudSyncPolicy,
  updateCloudSyncPolicy,
  type CloudDataSummary,
  type CloudIdentity,
  type CloudSyncPolicy,
} from "../../api/cloud";

export interface SyncCategoryPolicy {
  key: string;
  name: string;
  description: string;
  enabled: boolean;
  prohibitedNotice: string;
}

export const DEFAULT_SYNC_CATEGORIES: SyncCategoryPolicy[] = [
  {
    key: "conversations",
    name: "对话与消息",
    description: "同步用户与助理的消息历史。严格限定仅上传 user 与 assistant 角色。",
    enabled: true,
    prohibitedNotice: "内部 system 消息、开发调试提示与工具原始执行参数永不同步",
  },
  {
    key: "memories",
    name: "长期记忆",
    description: "同步用户已确认的偏好与关键记忆，支持多设备连续体验。",
    enabled: true,
    prohibitedNotice: "未被选中的临时上下文与未知嵌套元数据不进入同步",
  },
  {
    key: "agent_profiles",
    name: "智能体配置",
    description: "同步自定义智能体名称、描述、用户设定的 instructions 与可用能力清单。",
    enabled: true,
    prohibitedNotice: "API 密钥、Token、环境变量与展开后的系统提示词禁止同步",
  },
  {
    key: "task_graphs",
    name: "任务图谱与画布",
    description: "同步结构化任务节点、边引用与画布排版视图。",
    enabled: true,
    prohibitedNotice: "原始节点执行日志、执行中进程上下文不上传",
  },
  {
    key: "workflow_metadata",
    name: "工作流定义",
    description: "同步工作流节点与流转结构。",
    enabled: true,
    prohibitedNotice: "节点中的嵌入凭据与运行期临时中间输出不上传",
  },
  {
    key: "artifact_metadata",
    name: "工件元数据",
    description: "同步生成工件的名称、类型、MIME、大小与摘要。",
    enabled: true,
    prohibitedNotice: "本机绝对路径与文件原始字节禁止存入关系同步表",
  },
  {
    key: "capability_usage",
    name: "插件与能力使用历史",
    description: "同步安全网关执行状态、耗时、审批需求与风险级别指标。",
    enabled: true,
    prohibitedNotice: "禁止同步完整输入参数与执行结果，保护数据隐私",
  },
  {
    key: "settings",
    name: "用户界面偏好",
    description: "仅同步已审核的 system_preferences 界面与产品偏好段落。",
    enabled: true,
    prohibitedNotice: "模型、语音、搜索凭据和运行授权不属于该同步段落",
  },
];

export interface SyncPrivacyPanelProps {
  initialCategories?: SyncCategoryPolicy[];
  onPolicyChange?: (policy: Record<string, boolean>) => void;
}

export const SyncPrivacyPanel: React.FC<SyncPrivacyPanelProps> = ({
  initialCategories = DEFAULT_SYNC_CATEGORIES,
  onPolicyChange,
}) => {
  const [categories, setCategories] = useState<SyncCategoryPolicy[]>(initialCategories);
  const [summary, setSummary] = useState<CloudDataSummary | null>(null);
  const [identity, setIdentity] = useState<CloudIdentity | null>(null);
  const [loadingSummary, setLoadingSummary] = useState(false);
  const [summaryError, setSummaryError] = useState<string>("");
  const [savingPolicy, setSavingPolicy] = useState(false);

  const fetchSummary = async () => {
    setLoadingSummary(true);
    setSummaryError("");
    const identityState = await getCloudIdentity();
    if (!identityState || identityState.identity.status === "guest") {
      setIdentity(identityState?.identity ?? null);
      setSummary({ counts: {}, object_bytes: 0 });
      setLoadingSummary(false);
      return;
    }
    const result = await getPrivacySummary();
    if (result.ok) {
      setSummary(result.data.summary);
      setIdentity(result.data.identity);
    } else {
      setSummaryError(result.error);
    }
    setLoadingSummary(false);
  };

  useEffect(() => {
    void fetchSummary();
    void getCloudSyncPolicy().then((result) => {
      if (!result.ok) {
        setSummaryError(result.error);
        return;
      }
      setCategories(categoriesFromPolicy(result.data.policy));
    });
  }, []);

  const handleToggleCategory = async (key: string) => {
    const updated = categories.map((cat) =>
      cat.key === key ? { ...cat, enabled: !cat.enabled } : cat
    );
    setCategories(updated);
    setSavingPolicy(true);
    const policy = policyFromCategories(updated);
    const result = await updateCloudSyncPolicy(policy);
    setSavingPolicy(false);
    if (!result.ok) {
      setCategories(categories);
      setSummaryError(`同步范围保存失败：${result.error}`);
      return;
    }
    setSummaryError("");

    if (onPolicyChange) {
      const record: Record<string, boolean> = {};
      for (const cat of updated) {
        record[cat.key] = cat.enabled;
      }
      onPolicyChange(record);
    }
  };

  return (
    <div className="space-y-6" data-testid="sync-privacy-panel">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-[var(--border-soft)] pb-4">
        <div>
          <h2 className="text-xl font-bold text-[var(--foreground)] flex items-center gap-2">
            <Shield className="w-5 h-5 text-[var(--accent)]" />
            云同步隐私与数据保护
          </h2>
          <p className="text-sm text-[var(--foreground-muted)] mt-1">
            严格类型白名单与数据最小化控制。关闭分类不同步，关闭同步不推导删除。
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Badge tone="success" className="text-xs">
            本地优先
          </Badge>
          <Badge tone="accent" className="text-xs">
            严格白名单
          </Badge>
          <Badge tone="default" className="text-xs">
            Fail-Closed
          </Badge>
        </div>
      </div>

      {/* Principle Banner */}
      <Panel className="border-l-4 border-l-[var(--accent)] bg-[var(--surface-subtle)] space-y-2">
        <div className="flex items-start gap-3">
          <Info className="w-5 h-5 text-[var(--accent)] shrink-0 mt-0.5" />
          <div className="text-sm space-y-1">
            <p className="font-medium text-[var(--foreground)]">
              数据最小化与自由文本边界声明
            </p>
            <p className="text-[var(--foreground-muted)]">
              系统对所有同步实体采用<strong>严格 DTO 白名单与未识别字段拒绝 (deny_unknown_fields)</strong> 机制。内部系统提示词、开发者指令、凭据密钥与工具原始执行结果从不进入同步。
            </p>
            <p className="text-[var(--foreground-muted)] text-xs">
              注意：白名单机制无法识别用户在对话正文中自行粘贴的秘密。请通过以下类别开关精确控制上传范围；<strong>关闭某分类同步不会删除本地或云端既有数据</strong>，也不产生删除墓碑。
            </p>
          </div>
        </div>
      </Panel>

      {/* Category Toggles */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <h3 className="text-base font-semibold text-[var(--foreground)] flex items-center gap-2">
            <Sliders className="w-4 h-4 text-[var(--accent)]" />
            同步数据类别控制
          </h3>
          <span className="text-xs text-[var(--foreground-muted)]">
            已启用 {categories.filter((c) => c.enabled).length} / {categories.length} 项
          </span>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {categories.map((category) => (
            <Panel
              key={category.key}
              className={`transition-all duration-200 border ${
                category.enabled
                  ? "border-[var(--border-soft)] hover:border-[var(--accent)]"
                  : "border-[var(--border-subtle)] opacity-75 bg-[var(--surface-subtle)]"
              }`}
            >
              <div className="flex items-start justify-between gap-3">
                <div className="space-y-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-sm text-[var(--foreground)] truncate">
                      {category.name}
                    </span>
                    <Badge
                      tone={category.enabled ? "success" : "default"}
                      className="text-[11px] px-1.5 py-0"
                    >
                      {category.enabled ? "同步开启" : "仅保留本地"}
                    </Badge>
                  </div>
                  <p className="text-xs text-[var(--foreground-muted)] leading-relaxed">
                    {category.description}
                  </p>
                  <p className="text-[11px] text-[var(--danger,red)] flex items-center gap-1 pt-1">
                    <EyeOff className="w-3 h-3 shrink-0" />
                    <span>{category.prohibitedNotice}</span>
                  </p>
                </div>
                <button
                  type="button"
                  role="switch"
                  aria-checked={category.enabled}
                  onClick={() => void handleToggleCategory(category.key)}
                  disabled={savingPolicy}
                  className={`shrink-0 w-11 h-6 rounded-full transition-colors relative focus:outline-none focus:ring-2 focus:ring-[var(--accent)] ${
                    category.enabled ? "bg-[var(--accent)]" : "bg-[var(--border-strong)]"
                  }`}
                >
                  <span
                    className={`block w-4 h-4 bg-white rounded-full transition-transform absolute top-1 ${
                      category.enabled ? "left-6" : "left-1"
                    }`}
                  />
                </button>
              </div>
            </Panel>
          ))}
        </div>
      </div>

      {/* Security Gateway & Fail-Closed Guarantee */}
      <Panel className="space-y-3">
        <h3 className="text-sm font-semibold text-[var(--foreground)] flex items-center gap-2">
          <Lock className="w-4 h-4 text-[var(--accent)]" />
          安全网关与授权解析 (Fail-Closed)
        </h3>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs">
          <div className="p-3 rounded-[var(--radius-md)] bg-[var(--surface-subtle)] border border-[var(--border-soft)] space-y-1">
            <div className="font-medium text-[var(--foreground)] flex items-center gap-1.5">
              <CheckCircle2 className="w-3.5 h-3.5 text-[var(--success)]" />
              插件使用闭环 (Capability Usage)
            </div>
            <p className="text-[var(--foreground-muted)]">
              能力执行经网关写入使用记录 (状态、耗时、审批与风险级别)。不包含原始参数与输出，不使用易泄漏正文的哈希摘要。
            </p>
          </div>
          <div className="p-3 rounded-[var(--radius-md)] bg-[var(--surface-subtle)] border border-[var(--border-soft)] space-y-1">
            <div className="font-medium text-[var(--foreground)] flex items-center gap-1.5">
              <AlertTriangle className="w-3.5 h-3.5 text-[var(--warning)]" />
              授权记录损坏即阻断 (Fail-Closed)
            </div>
            <p className="text-[var(--foreground-muted)]">
              所有授权字段 (effect, permission, resource, source) 均采用带类型严格解析。任何异常损坏均报错并拒绝执行，永不默认降级为放行。
            </p>
          </div>
        </div>
      </Panel>

      {/* Cloud Data Summary Section */}
      <Panel className="space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Database className="w-4 h-4 text-[var(--accent)]" />
            <h3 className="text-sm font-semibold text-[var(--foreground)]">
              云端存储数据概览
            </h3>
            {identity?.account_id && (
              <Badge tone="default" className="text-xs">
                账号: {identity.email || identity.account_id}
              </Badge>
            )}
          </div>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void fetchSummary()}
            disabled={loadingSummary}
            className="text-xs h-7 gap-1"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loadingSummary ? "animate-spin" : ""}`} />
            刷新概览
          </Button>
        </div>

        {loadingSummary && !summary ? (
          <div className="flex items-center justify-center py-6 text-sm text-[var(--foreground-muted)]">
            <Spinner className="w-4 h-4 mr-2" />
            正在加载云端数据统计...
          </div>
        ) : summaryError ? (
          <div className="p-3 text-xs text-[var(--danger)] bg-[var(--surface-subtle)] rounded-[var(--radius-md)] border border-[var(--danger)]">
            云端概览加载失败：{summaryError}
          </div>
        ) : summary ? (
          <div className="space-y-3">
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-xs">
              {Object.entries(summary.counts).map(([type, count]) => (
                <div
                  key={type}
                  className="p-2.5 rounded-[var(--radius-md)] bg-[var(--surface-subtle)] border border-[var(--border-soft)] text-center"
                >
                  <div className="text-[var(--foreground-muted)] truncate">{type}</div>
                  <div className="text-base font-bold text-[var(--foreground)] mt-0.5">
                    {count}
                  </div>
                </div>
              ))}
            </div>
            <div className="text-xs text-[var(--foreground-muted)] flex items-center justify-between pt-1">
              <span>云端对象存储占用：{(summary.object_bytes / 1024).toFixed(1)} KB</span>
              <span>如需彻底清理云端数据，请前往账号管理提交注销或导出请求。</span>
            </div>
          </div>
        ) : (
          <div className="text-xs text-[var(--foreground-muted)] py-2">
            暂无云端数据统计信息。登录云账号后可查询云端同步摘要。
          </div>
        )}
      </Panel>
    </div>
  );
};

export function policyFromCategories(categories: SyncCategoryPolicy[]): CloudSyncPolicy {
  const enabled = new Map(categories.map((category) => [category.key, category.enabled]));
  return {
    sync_conversations: enabled.get("conversations") ?? false,
    sync_messages: enabled.get("conversations") ?? false,
    sync_memories: enabled.get("memories") ?? false,
    sync_agent_profiles: enabled.get("agent_profiles") ?? false,
    sync_workflow_metadata: enabled.get("workflow_metadata") ?? false,
    sync_artifact_metadata: enabled.get("artifact_metadata") ?? false,
    sync_task_graphs: enabled.get("task_graphs") ?? false,
    sync_canvas_views: enabled.get("task_graphs") ?? false,
    sync_settings: enabled.get("settings") ?? false,
    sync_capability_usage: enabled.get("capability_usage") ?? false,
  };
}

export function categoriesFromPolicy(policy: CloudSyncPolicy): SyncCategoryPolicy[] {
  return DEFAULT_SYNC_CATEGORIES.map((category) => {
    const enabled = category.key === "conversations"
      ? policy.sync_conversations && policy.sync_messages
      : category.key === "task_graphs"
        ? policy.sync_task_graphs && policy.sync_canvas_views
        : mappedKeyValue(policy, category.key);
    return { ...category, enabled };
  });
}

function mappedKeyValue(policy: CloudSyncPolicy, key: string): boolean {
  const values: Record<string, boolean> = {
    memories: policy.sync_memories,
    agent_profiles: policy.sync_agent_profiles,
    workflow_metadata: policy.sync_workflow_metadata,
    artifact_metadata: policy.sync_artifact_metadata,
    capability_usage: policy.sync_capability_usage,
    settings: policy.sync_settings,
  };
  return values[key] ?? false;
}
