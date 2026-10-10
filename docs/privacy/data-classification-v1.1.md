# YiLianQianYan v1.1 Data Classification & Sync Privacy Policy

> Version: 1.1 (Phase S6 Data Minimization & Security Hardening)  
> Scope: Public Client (`yilian-backend`), Frontend (`frontend`), and Cloud Integration (`YiLianCloud`)

---

## 1. Data Classification Matrix

| Category | Default Location | Default Sync | Cloud DTO Representation | Prohibited & Excluded Content |
|---|---|---:|---|---|
| **Account and device** | Cloud authoritative | Yes after login | Profile, status, device metadata | Provider secrets, sensitive local paths |
| **Agent Profile** | Local-first | User choice | Strict DTO (`SyncAgentProfilePayload`): name, description, user instructions, tool IDs, model ID, limits | Provider API keys, tokens, environment variables, runtime expanded system prompts, unverified secret refs |
| **Conversation & Message** | Local-first | User choice | Strict DTO (`SyncConversationPayload`, `SyncMessagePayload`): user & assistant messages only | System/developer prompts, hidden credentials, raw tool execution parameters/results, internal prompt templates |
| **Memory** | Local-first | User choice | Strict DTO (`SyncMemoryPayload`): content, category, source, timestamps | Unselected memories, raw metadata maps with unknown structures |
| **Capability Usage** | Local-first | User choice | Strict DTO (`SyncCapabilityUsagePayload`): capability_id, plugin_id, status, latency_ms, approval, risk | Full execution inputs/outputs, raw tool args, low-entropy digest hashes |
| **TaskGraph & CanvasView** | Local-first | User choice | Strict DTO (`SyncTaskGraphPayload`, `SyncCanvasViewPayload`): typed graphs, layouts, revisions | Raw node states, intermediate execution outputs, unreviewed opaque blobs |
| **Workflow Metadata** | Local-first | User choice | Strict DTO (`SyncWorkflowMetadataPayload`): definition metadata, typed node/edge refs | Running process states, inlined secrets, full execution output |
| **Artifact Metadata** | Local-first | User choice | Strict DTO (`SyncArtifactMetadataPayload`): name, type, MIME, size, summary, object reference | Absolute local filesystem paths, file byte contents, download tokens |
| **Binary Content** | Local resource store | Per item | Object storage references | Direct database BLOB insertion |
| **Provider Credentials** | OS SecretStore | Never | `configured`, `local_only`, `required_on_device` status only | Plaintext secrets, portable SecretRef payloads |
| **Security Audit & Grants** | Local-first & Gateway | Local-only (Audit: minimal) | Execution status, latency, risk level, grant effect/resource | Corrupt grant fallbacks, plain keys, full MCP payloads |

---

## 2. Strict Type Whitelisting & Data Minimization

1. **Deny Unknown Fields (`deny_unknown_fields`):**
   - Naive blacklist filtering (such as recursively deleting fields named `api_key`) is discarded as an unsafe primary defense.
   - All synchronization payloads are explicitly projected into typed DTOs on the client and strictly verified by the server schema validator. Any payload containing unrecognized fields or unsupported nested objects is rejected immediately (`INVALID_SYNC_PAYLOAD` / HTTP 422).
2. **Payload Size Limits:**
   - Every individual synchronization payload is capped at **512 KiB**. String lengths, identifier patterns, and nesting depths are validated independently prior to queueing or ingestion.
3. **Internal & System Message Exclusion:**
   - Synchronization filters out internal messages at capture time. Only `role = "user"` and `role = "assistant"` messages are eligible for synchronization. System prompts, runtime instructions, and tool call traces never leave the local environment.
4. **User-Entered Free Text Boundary:**
   - Schema validation and DTO whitelisting cannot guarantee detection of secrets manually pasted into prompt or conversation text by the user.
   - Users maintain granular control over synchronization categories via the Sync Privacy Panel. Disabling a category halts all synchronization for that data type without affecting local availability.

---

## 3. Tombstone vs. Policy Separation

1. **Policy Exclusion vs. Deletion:**
   - Toggling off synchronization for a category (e.g., disabling conversation or memory sync) **never** infers deletion and **never** generates an outbox tombstone (`operation = 'delete'`).
   - Turning off sync preserves both local state and existing remote backups. Remote data cleanup requires an explicit user lifecycle action (such as account deletion or cloud data export/purge).
2. **Transactional Local Deletions:**
   - Tombstone records (`cloud_sync_outbox` with `operation = 'delete'`) are recorded **strictly inside the same SQLite transaction** as the local business deletion (`record_local_deletion_tx`).
   - Outbox deletions always carry a `null` payload while validating the entity type against the supported model.

---

## 4. Capability Usage Lifecycle

1. **Execution Gateway Boundary:**
   - Every capability invocation passing through `SecurityExecutionGateway` produces an immutable `UsageRecord` upon completion, denial, or cancellation.
   - Records capture `execution_id`, `capability_id`, `plugin_id`, `version`, `execution_status`, `latency_ms`, `approval_required`, and `risk_level`.
2. **Privacy Guarantees:**
   - Full tool arguments and output streams are excluded from usage records.
   - Result digests are omitted by default to avoid accidental leakage of low-entropy plaintexts via hashing.
3. **Idempotent Cloud Projection:**
   - Remote capability usage sync projections are idempotent based on `execution_id`, preventing duplicate usage counting during network retries.

---

## 5. Fail-Closed Security Grants

1. **Strict Type Decoders:**
   - Database grant decoders for `effect`, `permission`, `resource`, and `source` reject lenient fallback conversions.
   - Any corrupt or invalid value produces a typed `FromSqlConversionFailure`, resulting in `GRANT_RECORD_INVALID`.
2. **Gateway Fail-Closed Enforcement:**
   - When grant records are invalid or unreadable, the security gateway fails closed: the requested tool or capability execution is denied immediately.
   - Unrecognized data states never grant or expand user or tool privileges.

---

## 6. Retention and Deletion

- Each synchronized entity maintains owner identification, revision numbering, and lifecycle states.
- Account deletion covers relational rows, object storage, search index entries, and active sessions.
- Minimal audit records may be retained under strict compliance deadlines without conversation or content payloads.

---

## 7. Logging & Diagnostics

- Diagnostic telemetry is restricted to request IDs, entity IDs, status codes, execution latency, and bounded error categorizations.
- Logs strictly exclude tokens, passwords, API keys, memory content, message bodies, tool arguments, and raw AI responses.
