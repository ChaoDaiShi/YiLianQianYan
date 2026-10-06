# YiLianQianYan v1.1 Data Classification

| Category | Default location | Default sync | Cloud representation | Prohibited content |
|---|---|---:|---|---|
| Account and device | Cloud authoritative | Yes after login | Profile, status, device metadata | Provider secrets, sensitive local paths |
| Agent Profile | Local-first | User choice | Revisioned payload | Secret values |
| Conversation and message | Local-first | User choice | Append-only message records | Hidden system credentials |
| Memory | Local-first | User choice | Revisioned record and tombstone | Unselected local memories |
| Capability usage | Local-first | User choice | Capability ID, version, status, latency, digest | Full arguments and tool result by default |
| TaskGraph and CanvasView | Local-first | User choice | Existing graph revision and view revision | Local execution secrets |
| Workflow metadata | Local-first | User choice | Definition metadata and revision | Running process state |
| Artifact metadata | Local-first | User choice | Name, type, hash, size, object reference | Binary body in PostgreSQL |
| Binary content | Local resource store | Per item | Object storage | Database BLOB |
| Provider credentials | OS SecretStore | Never | `configured`, `local_only`, `required_on_device` | Secret plaintext and portable SecretRef |
| Security audit | Local and cloud service scoped | Minimal required | IDs, actions, status, digest, latency | Full conversation, full MCP input, secrets |

## Retention and deletion

Each cloud entity records an owner, revision, lifecycle state and deletion timestamp where applicable. Account deletion covers relational rows, object storage, search indexes, caches, sessions and derived records. A minimal content-free security audit may be retained only under a documented retention policy.

## Logging

Logs and metrics may include request ID, entity ID, status, digest, latency and bounded error category. They do not include access tokens, refresh tokens, passwords, API keys, full conversations, complete Memory content, file bodies, MCP arguments or raw provider responses that may contain generated credentials.
