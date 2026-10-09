# YiLian Incremental Sync Contract v1

## Entity envelope

Every synchronized mutation has this logical shape:

```json
{
  "change_id": "stable UUID",
  "entity_type": "memory",
  "entity_id": "stable entity ID",
  "base_revision": 3,
  "revision": 4,
  "operation": "upsert",
  "payload": {},
  "device_id": "device UUID",
  "changed_at": 1791260000000
}
```

`operation` is `upsert` or `delete`. Delete creates a tombstone; it does not erase change history immediately. Payloads are bounded and validated by entity type. They must not contain provider secrets, SecretRef values, complete MCP arguments or unrestricted file bytes.

## Push

The client sends ordered outbox changes with a request idempotency key. The server applies each change transactionally and returns one result per `change_id`: `applied`, `duplicate`, `conflict`, or `rejected`. A partial response is explicit; the client removes only applied or duplicate rows from its outbox.

Append-only entities accept a duplicate stable ID as idempotent. Revision entities require `base_revision` to match the authoritative revision. Tombstones reject stale upserts even when an offline device still has the old content.

## Pull

The client sends an opaque cursor and bounded limit. The server returns changes ordered by monotonically increasing server sequence and a next cursor. Applying a page and persisting its cursor is one local transaction. A failed page is replayable.

## Conflict handling

- `message`, `capability_usage`, `audit`: append-only and stable-ID idempotent.
- `memory`: preserve local and remote values in a durable conflict record.
- `agent_profile`: never silently overwrite a newer revision.
- `settings`: revision is scoped by section.
- `task_graph`: use the existing graph revision.
- `canvas_view`: use the existing `view_revision` and retain stale replay semantics.
- `delete`: tombstone wins over stale upsert.

Conflict resolution creates a new local outbox change based on the selected authoritative revision. Editing the conflict record alone never mutates either copy.

## Offline behavior

Local writes complete without cloud availability and enter the outbox when their data category is enabled. Retry uses bounded exponential backoff with visible next attempt, last error and last successful sync time. Authentication failures pause sync until the session is refreshed or replaced; they do not discard queued changes.
