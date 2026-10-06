# ADR 004 Logout and Account Deletion

状态：Accepted
日期：2026-10-06

## Decision

Logout ends an account session and does not delete cloud data. It removes local session credentials by default; local provider secrets and synchronized local copies are separate explicit cleanup choices.

Account deletion is a permanent cloud lifecycle. The service revokes all sessions, creates a tracked DeletionJob, removes or anonymizes user records, objects, indexes and caches, then records a minimal content-free audit result. The client may separately choose whether to clear local data and local secrets.

## Consequences

- Product text and API names never use logout and account deletion interchangeably.
- Deletion states are `queued`, `running`, `completed`, or `failed` and remain queryable through a restricted status token or active closing session.
- A completed deletion cannot be reversed through Admin and old sessions cannot recreate data.
