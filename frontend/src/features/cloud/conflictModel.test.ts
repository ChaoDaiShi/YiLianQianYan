import { describe, expect, it } from 'vitest';
import {
  ConflictItem,
  getAvailableChoices,
  getConflictDisplayStatus,
  isRemotePayloadMissing,
  isRestoreBlocked,
  validateResolutionChoice,
} from './conflictModel';

describe('conflictModel', () => {
  const baseConflict: ConflictItem = {
    id: 'conf-1',
    entity_type: 'conversation',
    entity_id: 'conv-100',
    local_revision: 3,
    remote_revision: 5,
    local_operation: 'upsert',
    remote_operation: 'upsert',
    local_payload: { id: 'conv-100', title: 'Local Title' },
    remote_payload: { id: 'conv-100', title: 'Remote Title' },
    conflict_version: 1,
    local_generation: 2,
    status: 'open',
    created_at: 1000,
  };

  it('detects restore blocked when remote deleted and local is upsert', () => {
    const tombstoneConflict: ConflictItem = {
      ...baseConflict,
      local_operation: 'upsert',
      remote_operation: 'delete',
      remote_payload: null,
    };

    expect(isRestoreBlocked(tombstoneConflict)).toBe(true);

    const validation = validateResolutionChoice(tombstoneConflict, 'keep_local');
    expect(validation.valid).toBe(false);
    expect(validation.errorCode).toBe('RESTORE_REQUIRES_NEW_ENTITY');

    // But KeepLocalAsCopy is allowed!
    const copyValidation = validateResolutionChoice(tombstoneConflict, 'keep_local_as_copy');
    expect(copyValidation.valid).toBe(true);
  });

  it('rejects UseRemote when remote upsert payload is missing or null', () => {
    const missingPayloadConflict: ConflictItem = {
      ...baseConflict,
      remote_operation: 'upsert',
      remote_payload: null,
    };

    expect(isRemotePayloadMissing(missingPayloadConflict)).toBe(true);

    const validation = validateResolutionChoice(missingPayloadConflict, 'use_remote');
    expect(validation.valid).toBe(false);
    expect(validation.errorCode).toBe('REMOTE_PAYLOAD_MISSING');
  });

  it('allows KeepLocal for local deletion against remote upsert', () => {
    const localDeleteConflict: ConflictItem = {
      ...baseConflict,
      local_operation: 'delete',
      local_payload: null,
      remote_operation: 'upsert',
    };

    expect(isRestoreBlocked(localDeleteConflict)).toBe(false);

    const validation = validateResolutionChoice(localDeleteConflict, 'keep_local');
    expect(validation.valid).toBe(true);

    // KeepLocalAsCopy cannot be used on a deletion
    const copyValidation = validateResolutionChoice(localDeleteConflict, 'keep_local_as_copy');
    expect(copyValidation.valid).toBe(false);
    expect(copyValidation.errorCode).toBe('CANNOT_COPY_DELETION');
  });

  it('provides correct status display presentations', () => {
    const openStatus = getConflictDisplayStatus(baseConflict);
    expect(openStatus.status).toBe('pending');
    expect(openStatus.label).toBe('待处理');

    const resolvingConflict: ConflictItem = {
      ...baseConflict,
      status: 'resolving',
    };
    const resolvingStatus = getConflictDisplayStatus(resolvingConflict);
    expect(resolvingStatus.status).toBe('resolving');
    expect(resolvingStatus.label).toBe('正在提交选择');

    const resolvedConflict: ConflictItem = {
      ...baseConflict,
      status: 'resolved',
    };
    const resolvedStatus = getConflictDisplayStatus(resolvedConflict);
    expect(resolvedStatus.status).toBe('resolved');
    expect(resolvedStatus.label).toBe('已解决');

    const missingRemoteConflict: ConflictItem = {
      ...baseConflict,
      remote_payload: null,
    };
    const missingStatus = getConflictDisplayStatus(missingRemoteConflict);
    expect(missingStatus.status).toBe('missing_remote_payload');
    expect(missingStatus.label).toBe('缺少云端内容');
  });

  it('disables choices with clear reasons in available choices', () => {
    const tombstoneConflict: ConflictItem = {
      ...baseConflict,
      remote_operation: 'delete',
      remote_payload: null,
      local_operation: 'upsert',
    };

    const choices = getAvailableChoices(tombstoneConflict);
    const keepLocal = choices.find((c) => c.choice === 'keep_local');
    const keepCopy = choices.find((c) => c.choice === 'keep_local_as_copy');
    const useRemote = choices.find((c) => c.choice === 'use_remote');

    expect(keepLocal?.enabled).toBe(false);
    expect(keepLocal?.disabledReason).toContain('不可直接恢复');

    expect(keepCopy?.enabled).toBe(true);
    expect(useRemote?.enabled).toBe(true);
  });
});
