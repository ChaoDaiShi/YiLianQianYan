import { useCallback, useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { Badge, Button } from "../../components/ui";
import {
  getPrivacySummary,
  getCloudSyncPolicy,
  updateCloudSyncPolicy,
  type CloudDataSummary,
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
    description: "你和助理发送的消息",
    enabled: true,
    prohibitedNotice: "系统消息、调试提示与工具原始参数",
  },
  {
    key: "memories",
    name: "长期记忆",
    description: "已确认的偏好与长期记忆",
    enabled: true,
    prohibitedNotice: "未选中的临时上下文与未知字段",
  },
  {
    key: "agent_profiles",
    name: "智能体配置",
    description: "名称、描述、自定义指令和可用能力",
    enabled: true,
    prohibitedNotice: "密钥、令牌、环境变量与展开后的系统提示词",
  },
  {
    key: "task_graphs",
    name: "任务图谱与画布",
    description: "任务节点、连接关系与画布布局",
    enabled: true,
    prohibitedNotice: "执行日志和运行中的进程信息",
  },
  {
    key: "workflow_metadata",
    name: "工作流定义",
    description: "工作流节点与流转关系",
    enabled: true,
    prohibitedNotice: "嵌入的凭据和临时运行结果",
  },
  {
    key: "artifact_metadata",
    name: "工件信息",
    description: "工件的名称、类型、大小和摘要",
    enabled: true,
    prohibitedNotice: "本机路径、文件正文与二进制内容",
  },
  {
    key: "capability_usage",
    name: "插件与能力使用记录",
    description: "执行状态、用时、审批要求和风险级别",
    enabled: true,
    prohibitedNotice: "完整输入参数与执行结果",
  },
  {
    key: "settings",
    name: "界面偏好",
    description: "可跨设备使用的界面和产品偏好",
    enabled: true,
    prohibitedNotice: "模型、语音、搜索和 MCP 凭据，以及运行授权",
  },
];

const DATA_TYPE_LABELS: Record<string, string> = {
  ...Object.fromEntries(DEFAULT_SYNC_CATEGORIES.map(({ key, name }) => [key, name])),
  conversation: "对话",
  conversations: "对话",
  message: "消息",
  messages: "消息",
  memory: "记忆",
  agent_profile: "智能体配置",
  task_graph: "任务图谱",
  canvas_view: "画布视图",
  canvas_views: "画布视图",
  capability_usage_record: "插件与能力使用记录",
};

interface SyncPrivacyPanelProps {
  canReadCloudData: boolean;
  initialCategories?: SyncCategoryPolicy[];
}

export function SyncPrivacyPanel({
  canReadCloudData,
  initialCategories,
}: SyncPrivacyPanelProps) {
  const [categories, setCategories] = useState(initialCategories ?? []);
  const [summary, setSummary] = useState<CloudDataSummary | null>(null);
  const [loadingSummary, setLoadingSummary] = useState(canReadCloudData);
  const [summaryError, setSummaryError] = useState("");
  const [policyError, setPolicyError] = useState("");
  const [savingPolicy, setSavingPolicy] = useState(false);

  const fetchSummary = useCallback(async () => {
    setLoadingSummary(true);
    setSummaryError("");
    const result = await getPrivacySummary();
    if (result.ok) {
      setSummary(result.data.summary);
    } else {
      setSummaryError(result.error);
    }
    setLoadingSummary(false);
  }, []);

  useEffect(() => {
    if (canReadCloudData) void fetchSummary();
  }, [canReadCloudData, fetchSummary]);

  useEffect(() => {
    void getCloudSyncPolicy().then((result) => {
      if (!result.ok) {
        setPolicyError(`无法读取同步范围：${result.error}`);
        return;
      }
      setCategories(categoriesFromPolicy(result.data.policy));
    });
  }, []);

  const handleToggleCategory = async (key: string) => {
    const previousCategories = categories;
    const updated = categories.map((category) =>
      category.key === key ? { ...category, enabled: !category.enabled } : category
    );
    setCategories(updated);
    setSavingPolicy(true);
    setPolicyError("");
    const result = await updateCloudSyncPolicy(policyFromCategories(updated));
    if (!result.ok) {
      setCategories(previousCategories);
      setPolicyError(`同步范围保存失败：${result.error}`);
    }
    setSavingPolicy(false);
  };

  const policyReady = categories.length > 0;

  return (
    <div className="cloud-privacy-panel" data-testid="sync-privacy-panel">
      <section className="cloud-card" aria-labelledby="sync-scope-title">
        <div className="cloud-card-title">
          <div>
            <h3 id="sync-scope-title">同步范围</h3>
            <p>启用跨设备同步后才会按此范围同步。关闭分类会暂停上传和恢复，已有本地和云端数据会保留。</p>
          </div>
          {policyReady ? <Badge tone="info">允许 {categories.filter((category) => category.enabled).length} / {categories.length} 项</Badge> : null}
        </div>
        {policyError ? <p className="cloud-inline-error" role="alert">{policyError}</p> : null}
        {!policyReady && !policyError ? <p className="cloud-muted" role="status">正在读取同步范围…</p> : null}
        <ul className="cloud-policy-list">
          {categories.map((category) => (
            <li key={category.key}>
              <div>
                <strong>{category.name}</strong>
                <p>{category.description}。排除{category.prohibitedNotice}。</p>
              </div>
              <div className="cloud-policy-control">
                <span>{category.enabled ? "允许同步" : "不允许同步"}</span>
                <button
                  type="button"
                  role="switch"
                  aria-label={`允许同步${category.name}`}
                  aria-checked={category.enabled}
                  onClick={() => void handleToggleCategory(category.key)}
                  disabled={savingPolicy}
                  className="cloud-policy-switch focus-ring-token"
                ><span /></button>
              </div>
            </li>
          ))}
        </ul>
        <p className="cloud-muted">请勿将密钥等秘密粘贴到对话正文。允许同步对话时，正文会一并同步。</p>
      </section>

      {canReadCloudData ? (
        <section className="cloud-card" aria-labelledby="cloud-storage-title">
          <div className="cloud-card-title">
            <h3 id="cloud-storage-title">云端存储</h3>
            <Button variant="secondary" size="sm" onClick={() => void fetchSummary()} disabled={loadingSummary}>
              <RefreshCw />刷新概览
            </Button>
          </div>
          {loadingSummary ? <p className="cloud-muted" role="status">正在读取云端统计…</p> : null}
          {summaryError ? <p className="cloud-inline-error" role="alert">云端概览加载失败：{summaryError}</p> : null}
          {summary ? (
            <dl className="cloud-storage-list">
              {Object.entries(summary.counts).map(([type, count]) => (
                <div key={type}><dt>{DATA_TYPE_LABELS[type] ?? type}</dt><dd>{count} 项</dd></div>
              ))}
              <div><dt>对象存储占用</dt><dd>{(summary.object_bytes / 1024).toFixed(1)} KB</dd></div>
            </dl>
          ) : null}
        </section>
      ) : null}
    </div>
  );
}

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
