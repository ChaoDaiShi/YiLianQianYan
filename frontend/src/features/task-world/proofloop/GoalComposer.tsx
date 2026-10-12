import { useCallback, useRef, useState } from "react";
import { AlertCircle, CheckSquare, Sparkles, Square, X } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { createProofDraft, type ProofDraft } from "../../../api/proofloop";
import { Button, Textarea } from "../../../components/ui";

export interface AvailableCapabilityItem {
  id: string;
  name: string;
  description?: string;
}

export interface GoalComposerProps {
  workspaceId: string;
  initialGoal?: string;
  availableCapabilities?: AvailableCapabilityItem[];
  modelUnavailable?: boolean;
  onDraftCreated: (draft: ProofDraft) => void;
  onCancel?: () => void;
  className?: string;
}

export default function GoalComposer({
  workspaceId,
  initialGoal = "",
  availableCapabilities = [],
  modelUnavailable = false,
  onDraftCreated,
  onCancel,
  className = "",
}: GoalComposerProps) {
  const navigate = useNavigate();
  const [goal, setGoal] = useState(initialGoal);
  const [selectedCapabilities, setSelectedCapabilities] = useState<string[]>(() =>
    availableCapabilities.map((c) => c.id),
  );
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const abortControllerRef = useRef<AbortController | null>(null);

  const handleToggleCapability = (id: string) => {
    setSelectedCapabilities((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id],
    );
  };

  const handleSelectAllCapabilities = () => {
    setSelectedCapabilities(availableCapabilities.map((c) => c.id));
  };

  const handleClearCapabilities = () => {
    setSelectedCapabilities([]);
  };

  const handleCancelInFlight = () => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
    setLoading(false);
    setError("规划请求已由用户取消。");
  };

  const handleGenerate = useCallback(async () => {
    const trimmedGoal = goal.trim();
    if (!trimmedGoal) return;

    setLoading(true);
    setError(null);

    const controller = new AbortController();
    abortControllerRef.current = controller;

    try {
      const result = await createProofDraft(
        {
          workspace_id: workspaceId || "default",
          user_goal: trimmedGoal,
          allowed_capability_ids:
            selectedCapabilities.length > 0 ? selectedCapabilities : undefined,
        },
        controller.signal,
      );

      if (controller.signal.aborted) {
        return;
      }

      if (result.ok) {
        onDraftCreated(result.data);
      } else {
        setError(result.error.message || "生成计划草稿失败，请检查输入或配置后重试。");
      }
    } catch (err: unknown) {
      if (controller.signal.aborted) return;
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      if (abortControllerRef.current === controller) {
        abortControllerRef.current = null;
      }
      setLoading(false);
    }
  }, [goal, workspaceId, selectedCapabilities, onDraftCreated]);

  return (
    <div className={`goal-composer flex flex-col gap-4 rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface)] p-4 ${className}`}>
      <div className="flex items-center justify-between border-b border-[var(--border-soft)] pb-3">
        <div className="flex items-center gap-2">
          <Sparkles className="h-5 w-5 text-[var(--accent)]" />
          <h2 className="text-base font-semibold text-[var(--text)]">从目标生成可验证任务</h2>
        </div>
        {onCancel && (
          <button
            type="button"
            onClick={onCancel}
            aria-label="关闭目标输入"
            className="text-[var(--text-faint)] hover:text-[var(--text)]"
          >
            <X size={18} />
          </button>
        )}
      </div>

      {modelUnavailable && (
        <div className="flex items-start gap-2 rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-soft)] p-3 text-xs text-[var(--warning-fg)]">
          <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
          <div className="flex-1">
            <p className="font-medium">尚未配置可用的大模型服务</p>
            <p className="mt-0.5 text-[var(--text-secondary)]">
              自然语言目标编译依赖已配置的模型连接。您可以先配置模型，或手动创建画布节点。
            </p>
          </div>
          <Button
            size="sm"
            variant="secondary"
            onClick={() => navigate("/settings?section=model")}
          >
            前往设置
          </Button>
        </div>
      )}

      <div>
        <Textarea
          label="输入任务目标"
          value={goal}
          onChange={(e) => setGoal(e.target.value)}
          placeholder="例如：调研两种桌面本地持久化方案，对比写入性能与数据隔离约束，输出 Markdown 选型报告并列明引用资料来源。"
          rows={3}
          maxLength={4000}
          disabled={loading}
        />
        <p className="mt-1 text-xs text-[var(--text-faint)]">
          证行编译器将基于已注册工具的输出契约进行自动参数绑定与类型检查。计划需人工审查后保存，不会自动直接执行。
        </p>
      </div>

      {availableCapabilities.length > 0 && (
        <div className="flex flex-col gap-2 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-elevated)] p-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-[var(--text)]">
              允许使用的工具能力范围（已选 {selectedCapabilities.length} / {availableCapabilities.length}）
            </span>
            <div className="flex gap-2">
              <button
                type="button"
                onClick={handleSelectAllCapabilities}
                className="text-[11px] text-[var(--accent)] hover:underline"
              >
                全选
              </button>
              <button
                type="button"
                onClick={handleClearCapabilities}
                className="text-[11px] text-[var(--text-faint)] hover:underline"
              >
                清空
              </button>
            </div>
          </div>
          <div className="grid max-h-36 grid-cols-1 gap-1.5 overflow-y-auto pr-1 sm:grid-cols-2">
            {availableCapabilities.map((cap) => {
              const checked = selectedCapabilities.includes(cap.id);
              return (
                <button
                  key={cap.id}
                  type="button"
                  onClick={() => handleToggleCapability(cap.id)}
                  className={`flex items-center gap-2 rounded-[var(--radius-sm)] border px-2 py-1.5 text-left text-xs transition-colors ${
                    checked
                      ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--text)]"
                      : "border-[var(--border-soft)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
                  }`}
                >
                  {checked ? (
                    <CheckSquare className="h-3.5 w-3.5 shrink-0 text-[var(--accent)]" />
                  ) : (
                    <Square className="h-3.5 w-3.5 shrink-0 text-[var(--text-faint)]" />
                  )}
                  <div className="min-w-0 flex-1">
                    <span className="block truncate font-medium">{cap.name}</span>
                    {cap.description && (
                      <span className="block truncate text-[10px] text-[var(--text-faint)]">
                        {cap.description}
                      </span>
                    )}
                  </div>
                </button>
              );
            })}
          </div>
        </div>
      )}

      {error && (
        <div className="rounded-[var(--radius-md)] border border-[var(--danger-border)] bg-[var(--danger-soft)] p-3 text-xs text-[var(--danger-fg)]" role="alert">
          {error}
        </div>
      )}

      <div className="flex items-center justify-end gap-2 pt-2">
        {loading ? (
          <Button
            type="button"
            variant="secondary"
            onClick={handleCancelInFlight}
          >
            取消规划
          </Button>
        ) : (
          onCancel && (
            <Button type="button" variant="ghost" onClick={onCancel}>
              返回
            </Button>
          )
        )}
        <Button
          type="button"
          disabled={loading || !goal.trim() || modelUnavailable}
          onClick={() => void handleGenerate()}
        >
          {loading ? "正在编译候选计划…" : "生成任务计划"}
        </Button>
      </div>
    </div>
  );
}
