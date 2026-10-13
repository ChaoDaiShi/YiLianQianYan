import type { CloudConflictRecord, ConflictChoice } from '../../api/cloud';

export type ResolutionChoice = ConflictChoice;
export type ConflictItem = CloudConflictRecord;

export type ConflictDisplayStatus =
  | 'pending'
  | 'resolving'
  | 'resolved'
  | 'expired'
  | 'missing_remote_payload';

export interface ConflictStatusPresentation {
  status: ConflictDisplayStatus;
  label: string;
  tone: 'default' | 'success' | 'warning' | 'info' | 'danger';
  detail: string;
}

export interface ChoiceOption {
  choice: ResolutionChoice;
  label: string;
  description: string;
  enabled: boolean;
  disabledReason?: string;
}

/**
 * Returns true if the remote deleted this entity while local has an upsert.
 * Resurrecting the original UUID via KeepLocal is strictly forbidden.
 */
export function isRestoreBlocked(conflict: ConflictItem): boolean {
  return conflict.remote_operation === 'delete' && conflict.local_operation === 'upsert';
}

/**
 * Returns true if remote operation is an upsert but remote payload is missing or null.
 */
export function isRemotePayloadMissing(conflict: ConflictItem): boolean {
  return (
    conflict.remote_operation === 'upsert' &&
    (conflict.remote_payload === null || conflict.remote_payload === undefined)
  );
}

/**
 * Returns true if local operation is an upsert but local payload is missing or null.
 */
export function isLocalPayloadMissing(conflict: ConflictItem): boolean {
  return (
    conflict.local_operation === 'upsert' &&
    (conflict.local_payload === null || conflict.local_payload === undefined)
  );
}

/**
 * Get human-readable presentation of the conflict state.
 */
export function getConflictDisplayStatus(conflict: ConflictItem): ConflictStatusPresentation {
  if (conflict.status === 'resolving') {
    return {
      status: 'resolving',
      label: '选择已提交',
      tone: 'info',
      detail: '正在等待同步确认',
    };
  }

  if (conflict.status === 'resolved') {
    if (conflict.copied_entity_id) {
      return {
        status: 'resolved',
        label: '本地副本已创建',
        tone: 'success',
        detail: `副本 ${conflict.copied_entity_id} 已创建；上传状态请在同步中心查看并等待实际确认`,
      };
    }
    return {
      status: 'resolved',
      label: '本地冲突已处理',
      tone: 'success',
      detail: '此冲突记录已处理；这不代表所有设备间的数据都已一致',
    };
  }

  if (isRemotePayloadMissing(conflict)) {
    return {
      status: 'missing_remote_payload',
      label: '缺少云端内容',
      tone: 'warning',
      detail: '云端变更内容不完整，保持待处理状态',
    };
  }

  return {
    status: 'pending',
    label: '待处理',
    tone: 'warning',
    detail: '检测到本机与云端版本冲突，请选择解决方式',
  };
}

/**
 * Evaluates whether a chosen resolution is valid according to the tombstone and payload safety rules.
 */
export function validateResolutionChoice(
  conflict: ConflictItem,
  choice: ResolutionChoice
): { valid: boolean; errorCode?: string; errorMessage?: string } {
  if (choice === 'use_remote') {
    if (isRemotePayloadMissing(conflict)) {
      return {
        valid: false,
        errorCode: 'REMOTE_PAYLOAD_MISSING',
        errorMessage: '云端内容缺失，无法采用云端版本',
      };
    }
    return { valid: true };
  }

  if (choice === 'keep_local') {
    if (isRestoreBlocked(conflict)) {
      return {
        valid: false,
        errorCode: 'RESTORE_REQUIRES_NEW_ENTITY',
        errorMessage: '云端已删除该项，禁止直接恢复原标识；请选择“作为新副本保留”',
      };
    }
    if (isLocalPayloadMissing(conflict)) {
      return {
        valid: false,
        errorCode: 'LOCAL_PAYLOAD_MISSING',
        errorMessage: '本地新增或修改内容缺失',
      };
    }
    return { valid: true };
  }

  if (choice === 'keep_local_as_copy') {
    if (conflict.local_operation === 'delete') {
      return {
        valid: false,
        errorCode: 'CANNOT_COPY_DELETION',
        errorMessage: '删除操作无法保留为副本',
      };
    }
    if (isLocalPayloadMissing(conflict)) {
      return {
        valid: false,
        errorCode: 'LOCAL_PAYLOAD_MISSING',
        errorMessage: '本地内容缺失，无法创建副本',
      };
    }
    return { valid: true };
  }

  return { valid: true };
}

/**
 * Generates available resolution choices with detailed instructions and enable/disable states.
 */
export function getAvailableChoices(conflict: ConflictItem): ChoiceOption[] {
  const remoteMissing = isRemotePayloadMissing(conflict);
  const restoreBlocked = isRestoreBlocked(conflict);
  const localMissing = isLocalPayloadMissing(conflict);

  const choices: ChoiceOption[] = [];

  // 1. Use Remote
  choices.push({
    choice: 'use_remote',
    label: conflict.remote_operation === 'delete' ? '采用云端（接受删除）' : '采用云端版本',
    description:
      conflict.remote_operation === 'delete'
        ? '接受云端删除，本地数据将被标记删除'
        : '用云端内容覆盖本地当前版本',
    enabled: !remoteMissing,
    disabledReason: remoteMissing ? '云端数据缺失，无法采用' : undefined,
  });

  // 2. Keep Local
  choices.push({
    choice: 'keep_local',
    label: conflict.local_operation === 'delete' ? '保留本地（维持删除）' : '保留本地版本',
    description:
      conflict.local_operation === 'delete'
        ? '维持本地删除意图，向云端推送删除'
        : '用本地当前内容覆盖云端版本',
    enabled: !restoreBlocked && !localMissing,
    disabledReason: restoreBlocked
      ? '云端已删除此项，不可直接恢复原 ID；请使用“保留为新副本”'
      : localMissing
        ? '本地数据内容缺失'
        : undefined,
  });

  // 3. Keep Local As Copy (only relevant for upsert local changes)
  if (conflict.local_operation === 'upsert') {
    choices.push({
      choice: 'keep_local_as_copy',
      label: '保留为新副本',
      description: '为本地内容生成全新 ID 保存为独立副本，同时接受云端变更',
      enabled: !localMissing,
      disabledReason: localMissing ? '本地数据内容缺失' : undefined,
    });
  }

  return choices;
}
