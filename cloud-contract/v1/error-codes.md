# YiLian Cloud v1 Error Codes

| Code | HTTP | Category | Retryable |
|---|---:|---|---:|
| `INVALID_REQUEST` | 400 | permanent | No |
| `AUTH_REQUIRED` | 401 | auth | No |
| `TOKEN_EXPIRED` | 401 | auth | Yes after refresh |
| `ACCESS_DENIED` | 403 | auth | No |
| `NOT_FOUND` | 404 | permanent | No |
| `IDEMPOTENCY_KEY_REUSED` | 409 | conflict | No |
| `SYNC_CONFLICT` | 409 | conflict | No |
| `ACCOUNT_CLOSING` | 409 | permanent | No |
| `SCOPE_CHANGED` | 409 | conflict | No |
| `CHECKPOINT_CHANGED` | 409 | conflict | Yes after reload |
| `CURSOR_RESET_REQUIRED` | 409 | conflict | Yes after reset |
| `REVISION_CONTENT_MISMATCH` | 409 | conflict | No |
| `REMOTE_PAYLOAD_MISSING` | 409 | conflict | Yes after fetch |
| `CONFLICT_CHANGED` | 409 | conflict | Yes after reload |
| `DEVICE_OWNER_MISMATCH` | 409 | auth | No |
| `RESTORE_REQUIRES_NEW_ENTITY` | 409 | conflict | No |
| `EXPORT_EXPIRED` | 410 | permanent | No |
| `UNSUPPORTED_SCHEMA` | 422 | permanent | No |
| `INVALID_REVISION` | 422 | permanent | No |
| `INVALID_SYNC_PAYLOAD` | 422 | permanent | No |
| `GRANT_RECORD_INVALID` | 422 | permanent | No |
| `SYNC_UPGRADE_REQUIRED` | 426 | permanent | No |
| `RATE_LIMITED` | 429 | rate_limit | Yes |
| `INVALID_PAGE` | 502 | permanent | No |
| `PROVIDER_AUTH_FAILED` | 502 | permanent | No |
| `PROVIDER_UNAVAILABLE` | 502 | retryable | Yes |
| `SERVICE_UNAVAILABLE` | 503 | retryable | Yes |
| `JOB_LEASE_LOST` | 500 | retryable | Yes |
| `DELETE_OBJECT_FAILED` | 500 | retryable | Yes |

Clients must branch on `code` and `retryable`, not localized message text.

