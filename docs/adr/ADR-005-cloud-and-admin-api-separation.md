# ADR 005 Cloud API and Admin Separation

状态：Accepted
日期：2026-10-06

## Decision

The versioned public Cloud API contains only Identity, Session, Device, Sync, User Data metadata, Search Proxy, Export and Delete Request contracts needed by the desktop client. Admin operations live behind a separate private API rooted at `/internal/admin/*` and never appear in the public OpenAPI, generated client or desktop frontend types.

Admin responses are metadata-only by default. Complete conversation text, Memory content, uploaded binary content, MCP arguments, tool results and provider secrets are unavailable without a future user-authorized Support Access design.

## Consequences

- Public access tokens cannot call Admin routes.
- Admin authentication, RBAC and audit types remain private source.
- The desktop local backend never receives or stores an Admin credential.
