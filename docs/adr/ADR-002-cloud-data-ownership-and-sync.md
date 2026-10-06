# ADR 002 Cloud Data Ownership and Incremental Sync

状态：Accepted
日期：2026-10-06

## Decision

Users own synchronized data. Sync exchanges entity changes through an outbox and a server cursor. Every change carries a stable entity ID, entity type, revision, operation, originating device and idempotency key. Deletes use tombstones. The client never downloads a cloud database and replaces local SQLite.

Messages, capability usage and audit metadata are append-only. Memory, Agent Profile, Settings sections, TaskGraph, CanvasView and Workflow metadata use explicit revisions. A stale write becomes a conflict and cannot silently overwrite a newer revision. Tombstones win over stale device writes.

## Consequences

- Offline edits remain usable and queue for later push.
- Pull is cursor based and restartable.
- Conflicts are durable user-visible records.
- Existing graph revision and Canvas view revision semantics remain authoritative for those entity types.
