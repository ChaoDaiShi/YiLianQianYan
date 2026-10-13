import type {
  CloudConflictRecord,
  ConflictChoice,
  ResolveConflictChoicePayload,
} from '../../api/cloud';

export interface ConflictRequestScope {
  local_identity_id: string;
  account_id: string;
}

interface PendingConflictRequest extends ConflictRequestScope {
  conflict_id: string;
  expected_conflict_version: number;
  expected_remote_revision: number;
  choice: ConflictChoice;
  request_id: string;
}

const STORAGE_KEY = 'yilian.cloud.pending-conflict-requests.v1';
const memoryRequests = new Map<string, { payload: ResolveConflictChoicePayload; scopeKey: string }>();

function isPendingConflictRequest(value: unknown): value is PendingConflictRequest {
  if (typeof value !== 'object' || value === null) return false;
  const entry = value as Record<string, unknown>;
  return typeof entry.local_identity_id === 'string' &&
    typeof entry.account_id === 'string' &&
    typeof entry.conflict_id === 'string' &&
    typeof entry.expected_conflict_version === 'number' &&
    typeof entry.expected_remote_revision === 'number' &&
    (entry.choice === 'keep_local' || entry.choice === 'use_remote' || entry.choice === 'keep_local_as_copy') &&
    typeof entry.request_id === 'string';
}

export function buildConflictChoice(
  conflict: Pick<CloudConflictRecord, 'conflict_version' | 'remote_revision'>,
  choice: ConflictChoice,
  requestId: string,
): ResolveConflictChoicePayload {
  return {
    choice,
    expected_conflict_version: conflict.conflict_version,
    expected_remote_revision: conflict.remote_revision,
    request_id: requestId,
  };
}

function requestKey(
  scope: ConflictRequestScope,
  conflict: Pick<CloudConflictRecord, 'id' | 'conflict_version'>,
  choice: ConflictChoice,
): string {
  return JSON.stringify([
    scope.local_identity_id,
    scope.account_id,
    conflict.id,
    conflict.conflict_version,
    choice,
  ]);
}

function readPendingRequests(): PendingConflictRequest[] {
  if (typeof window === 'undefined') return [];
  try {
    const value: unknown = JSON.parse(window.sessionStorage.getItem(STORAGE_KEY) || '[]');
    if (!Array.isArray(value)) return [];
    return value.filter(isPendingConflictRequest);
  } catch {
    return [];
  }
}

function writePendingRequests(requests: PendingConflictRequest[]): void {
  if (typeof window === 'undefined') return;
  try {
    window.sessionStorage.setItem(STORAGE_KEY, JSON.stringify(requests));
  } catch {
    // The in-memory entry still keeps retries stable for this page session.
  }
}

export function getConflictChoiceRequest(
  scope: ConflictRequestScope,
  conflict: Pick<CloudConflictRecord, 'id' | 'conflict_version' | 'remote_revision'>,
  choice: ConflictChoice,
): ResolveConflictChoicePayload {
  const key = requestKey(scope, conflict, choice);
  const memory = memoryRequests.get(key);
  if (memory) return memory.payload;

  const stored = readPendingRequests();
  const existing = stored.find((entry) => requestKey(scope, {
    id: entry.conflict_id,
    conflict_version: entry.expected_conflict_version,
  }, entry.choice) === key);
  if (existing) {
    const payload: ResolveConflictChoicePayload = {
      choice: existing.choice,
      expected_conflict_version: existing.expected_conflict_version,
      expected_remote_revision: existing.expected_remote_revision,
      request_id: existing.request_id,
    };
    memoryRequests.set(key, {
      payload,
      scopeKey: JSON.stringify([scope.local_identity_id, scope.account_id]),
    });
    return payload;
  }

  const requestId = crypto.randomUUID();
  const payload = buildConflictChoice(conflict, choice, requestId);
  const entry: PendingConflictRequest = {
    ...scope,
    conflict_id: conflict.id,
    expected_conflict_version: conflict.conflict_version,
    expected_remote_revision: conflict.remote_revision,
    choice,
    request_id: requestId,
  };
  writePendingRequests([...stored, entry]);
  memoryRequests.set(key, {
    payload,
    scopeKey: JSON.stringify([scope.local_identity_id, scope.account_id]),
  });
  return payload;
}

export function clearConflictChoiceRequest(
  scope: ConflictRequestScope,
  conflict: Pick<CloudConflictRecord, 'id' | 'conflict_version'>,
  choice: ConflictChoice,
): void {
  const key = requestKey(scope, conflict, choice);
  memoryRequests.delete(key);
  writePendingRequests(readPendingRequests().filter((entry) => requestKey(scope, {
    id: entry.conflict_id,
    conflict_version: entry.expected_conflict_version,
  }, entry.choice) !== key));
}

export function retainConflictRequestsForScope(scope: ConflictRequestScope | null): void {
  const scopeKey = scope ? JSON.stringify([scope.local_identity_id, scope.account_id]) : null;
  const stored = readPendingRequests().filter((entry) =>
    scopeKey !== null && JSON.stringify([entry.local_identity_id, entry.account_id]) === scopeKey,
  );
  writePendingRequests(stored);
  for (const [key, entry] of memoryRequests) {
    if (entry.scopeKey !== scopeKey) memoryRequests.delete(key);
  }
}
