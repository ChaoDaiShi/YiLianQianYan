import React, { useState } from 'react';
import {
  ChoiceOption,
  ConflictItem,
  getAvailableChoices,
  getConflictDisplayStatus,
  ResolutionChoice,
  validateResolutionChoice,
} from './conflictModel';

export interface ConflictResolutionPanelProps {
  conflicts: ConflictItem[];
  onResolve: (
    conflict: ConflictItem,
    choice: ResolutionChoice
  ) => Promise<{ ok: boolean; message?: string }>;
  onRefresh?: () => void;
  isLoading?: boolean;
}

export const ConflictResolutionPanel: React.FC<ConflictResolutionPanelProps> = ({
  conflicts,
  onResolve,
  onRefresh,
  isLoading = false,
}) => {
  const [resolvingId, setResolvingId] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<{ [id: string]: string }>({});

  const handleChoiceClick = async (conflict: ConflictItem, choice: ResolutionChoice) => {
    const validation = validateResolutionChoice(conflict, choice);
    if (!validation.valid) {
      setErrorMsg((prev) => ({
        ...prev,
        [conflict.id]: validation.errorMessage || '选择不合法',
      }));
      return;
    }

    setResolvingId(conflict.id);
    setErrorMsg((prev) => {
      const copy = { ...prev };
      delete copy[conflict.id];
      return copy;
    });

    try {
      const res = await onResolve(conflict, choice);
      if (!res.ok) {
        setErrorMsg((prev) => ({
          ...prev,
          [conflict.id]: res.message || '解决冲突失败',
        }));
      }
    } catch (e: any) {
      setErrorMsg((prev) => ({
        ...prev,
        [conflict.id]: e?.message || '网络或处理异常',
      }));
    } finally {
      setResolvingId(null);
    }
  };

  if (conflicts.length === 0) {
    return (
      <div className="p-6 text-center bg-white dark:bg-zinc-900 rounded-xl border border-zinc-200 dark:border-zinc-800 shadow-sm">
        <div className="text-zinc-400 dark:text-zinc-500 text-3xl mb-2">✓</div>
        <h3 className="text-base font-medium text-zinc-900 dark:text-zinc-100">无冲突项目</h3>
        <p className="text-sm text-zinc-500 dark:text-zinc-400 mt-1">
          当前所有云端与本地数据版本完全一致，无需手动干预。
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
            同步冲突解决 ({conflicts.length})
          </h2>
          <p className="text-xs text-zinc-500 dark:text-zinc-400 mt-0.5">
            请对比两侧内容后选择合并策略。若云端已删除，本地修改将禁止直接复活原 ID，可选择保留为副本。
          </p>
        </div>
        {onRefresh && (
          <button
            onClick={onRefresh}
            disabled={isLoading}
            className="px-3 py-1.5 text-xs font-medium text-zinc-700 dark:text-zinc-200 bg-zinc-100 dark:bg-zinc-800 hover:bg-zinc-200 dark:hover:bg-zinc-700 rounded-md transition-colors"
          >
            {isLoading ? '刷新中...' : '刷新状态'}
          </button>
        )}
      </div>

      <div className="space-y-4">
        {conflicts.map((conflict) => {
          const presentation = getConflictDisplayStatus(conflict);
          const choices = getAvailableChoices(conflict);
          const isCurrentResolving = resolvingId === conflict.id || conflict.status === 'resolving';
          const localError = errorMsg[conflict.id] || conflict.error_message;

          return (
            <div
              key={conflict.id}
              className="bg-white dark:bg-zinc-900 rounded-xl border border-zinc-200 dark:border-zinc-800 p-5 shadow-sm space-y-4"
            >
              {/* Card Header */}
              <div className="flex items-start justify-between">
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-semibold text-sm text-zinc-900 dark:text-zinc-100 uppercase tracking-wider">
                      {conflict.entity_type}
                    </span>
                    <span className="text-xs text-zinc-400 font-mono">
                      {conflict.entity_id}
                    </span>
                  </div>
                  <div className="text-xs text-zinc-500 mt-1">
                    冲突版本: v{conflict.conflict_version} · 本地代数: gen{conflict.local_generation}
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <span
                    className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${
                      presentation.tone === 'success'
                        ? 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300'
                        : presentation.tone === 'info'
                        ? 'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-300'
                        : presentation.tone === 'warning'
                        ? 'bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-300'
                        : 'bg-zinc-100 text-zinc-800 dark:bg-zinc-800 dark:text-zinc-300'
                    }`}
                  >
                    {presentation.label}
                  </span>
                </div>
              </div>

              {/* Error banner if present */}
              {localError && (
                <div className="p-3 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800/50 rounded-lg text-xs text-red-700 dark:text-red-400">
                  {localError}
                </div>
              )}

              {/* Side-by-Side Payload Comparison */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {/* Local Column */}
                <div className="p-3 bg-zinc-50 dark:bg-zinc-950/50 rounded-lg border border-zinc-100 dark:border-zinc-800/80">
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-xs font-semibold text-zinc-700 dark:text-zinc-300">
                      本地版本 (rev {conflict.local_revision})
                    </span>
                    <span
                      className={`text-xs px-1.5 py-0.5 rounded font-mono ${
                        conflict.local_operation === 'delete'
                          ? 'bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-400'
                          : 'bg-zinc-200 dark:bg-zinc-800 text-zinc-700 dark:text-zinc-300'
                      }`}
                    >
                      {conflict.local_operation}
                    </span>
                  </div>
                  <pre className="text-xs font-mono bg-white dark:bg-zinc-900 p-2.5 rounded border border-zinc-200 dark:border-zinc-800 overflow-x-auto max-h-48 text-zinc-800 dark:text-zinc-200">
                    {conflict.local_payload
                      ? JSON.stringify(conflict.local_payload, null, 2)
                      : '(无内容 / 已标记删除)'}
                  </pre>
                </div>

                {/* Remote Column */}
                <div className="p-3 bg-zinc-50 dark:bg-zinc-950/50 rounded-lg border border-zinc-100 dark:border-zinc-800/80">
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-xs font-semibold text-zinc-700 dark:text-zinc-300">
                      云端版本 (rev {conflict.remote_revision})
                    </span>
                    <span
                      className={`text-xs px-1.5 py-0.5 rounded font-mono ${
                        conflict.remote_operation === 'delete'
                          ? 'bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-400'
                          : 'bg-zinc-200 dark:bg-zinc-800 text-zinc-700 dark:text-zinc-300'
                      }`}
                    >
                      {conflict.remote_operation}
                    </span>
                  </div>
                  <pre className="text-xs font-mono bg-white dark:bg-zinc-900 p-2.5 rounded border border-zinc-200 dark:border-zinc-800 overflow-x-auto max-h-48 text-zinc-800 dark:text-zinc-200">
                    {conflict.remote_payload
                      ? JSON.stringify(conflict.remote_payload, null, 2)
                      : conflict.remote_operation === 'delete'
                      ? '(云端已删除 - Tombstone)'
                      : '(云端内容未获取)'}
                  </pre>
                </div>
              </div>

              {/* Action Choices */}
              <div className="pt-2 border-t border-zinc-100 dark:border-zinc-800">
                {conflict.status === 'resolved' ? (
                  <div className="text-xs text-green-600 dark:text-green-400 font-medium">
                    ✓ 该冲突已解决
                  </div>
                ) : (
                  <div className="flex flex-wrap items-center gap-3">
                    {choices.map((opt: ChoiceOption) => (
                      <div key={opt.choice} className="relative group">
                        <button
                          onClick={() => handleChoiceClick(conflict, opt.choice)}
                          disabled={!opt.enabled || isCurrentResolving}
                          className={`px-3.5 py-2 text-xs font-medium rounded-lg transition-colors ${
                            opt.enabled && !isCurrentResolving
                              ? opt.choice === 'keep_local_as_copy'
                                ? 'bg-indigo-600 text-white hover:bg-indigo-700 shadow-sm'
                                : opt.choice === 'keep_local'
                                ? 'bg-zinc-800 text-white dark:bg-zinc-200 dark:text-zinc-900 hover:bg-zinc-700 dark:hover:bg-white'
                                : 'bg-zinc-100 text-zinc-800 dark:bg-zinc-800 dark:text-zinc-200 hover:bg-zinc-200 dark:hover:bg-zinc-700'
                              : 'bg-zinc-100 dark:bg-zinc-800/50 text-zinc-400 dark:text-zinc-600 cursor-not-allowed'
                          }`}
                          title={opt.disabledReason || opt.description}
                        >
                          {isCurrentResolving && resolvingId === conflict.id ? '提交中...' : opt.label}
                        </button>
                        {opt.disabledReason && (
                          <div className="hidden group-hover:block absolute bottom-full mb-1 left-0 z-10 w-64 p-2 bg-zinc-900 text-white text-[11px] rounded shadow-lg">
                            {opt.disabledReason}
                          </div>
                        )}
                      </div>
                    ))}
                    {isCurrentResolving && (
                      <span className="text-xs text-zinc-500 animate-pulse">
                        正在提交选择至同步队列...
                      </span>
                    )}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
