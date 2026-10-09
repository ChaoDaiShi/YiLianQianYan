# YiLian Cloud Public Contract v1

## Purpose

This contract is the only shared boundary between the public desktop client and the private YiLian Cloud implementation. It is additive within `/api/v1`. Private Admin operations, internal job controls, database fields and deployment details are deliberately absent.

The canonical machine-readable contract is `cloud-contract/v1/openapi.yaml`.

## Authentication

Register and login create a device session. Access tokens authorize public user routes. Refresh tokens rotate on refresh and are revoked on reuse, logout, explicit session revoke or account deletion. The desktop stores both token values through SecretStore and stores only redacted session metadata in SQLite.

Invalid or missing credentials return `401`. Authenticated users without access to a resource receive `403`. Neither condition disables local desktop functionality.

## Idempotency

Registration retries, sync push, session revoke, export requests and deletion requests accept `Idempotency-Key`. Repeating the same key and request returns the original result. Reusing a key with a different request returns `IDEMPOTENCY_KEY_REUSED`.

## Errors

Every error uses:

```json
{
  "error": {
    "code": "SYNC_CONFLICT",
    "message": "The entity revision is stale.",
    "request_id": "request UUID",
    "retryable": false,
    "details": {}
  }
}
```

Stable categories are `auth`, `conflict`, `retryable`, `permanent` and `rate_limit`. Responses never include secret plaintext, password hashes, internal Admin fields or raw provider credential responses.

## Public resources

- Identity: register, login, refresh, current account.
- Sessions and devices: list, revoke one, revoke all.
- Sync: idempotent push and cursor-based pull.
- User data: metadata summary only; entity content travels through sync payloads selected by the user.
- Search Proxy: normalized query and result contract.
- Lifecycle: export request/status and account deletion request/status.
- Health and capabilities: unauthenticated service availability and supported public features.

## Compatibility

New optional fields and endpoints are additive. Removing or changing required fields requires a new API version. Cursor values are opaque. Clients must not infer ordering from local clocks; sync ordering uses server sequence and entity revision.
