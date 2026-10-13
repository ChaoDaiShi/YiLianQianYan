import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  buildConflictChoice,
  getConflictChoiceRequest,
  retainConflictRequestsForScope,
} from './conflictRequest';

afterEach(() => {
  retainConflictRequestsForScope(null);
  vi.unstubAllGlobals();
});

describe('conflict choice request', () => {
  it('keeps the copy choice and versions from the conflict record', () => {
    expect(buildConflictChoice(
      { conflict_version: 4, remote_revision: 12 },
      'keep_local_as_copy',
      'decision-1',
    )).toEqual({
      choice: 'keep_local_as_copy',
      expected_conflict_version: 4,
      expected_remote_revision: 12,
      request_id: 'decision-1',
    });
  });

  it('reuses the same request and recorded versions for a retry', () => {
    const randomUUID = vi.fn(() => 'decision-retry-1');
    vi.stubGlobal('crypto', { randomUUID });
    const scope = { local_identity_id: 'local-1', account_id: 'account-1' };
    const conflict = { id: 'conflict-1', conflict_version: 4, remote_revision: 12 };

    const first = getConflictChoiceRequest(scope, conflict, 'keep_local_as_copy');
    const retry = getConflictChoiceRequest(
      scope,
      { ...conflict, remote_revision: 13 },
      'keep_local_as_copy',
    );

    expect(retry).toEqual(first);
    expect(randomUUID).toHaveBeenCalledTimes(1);
  });
});
