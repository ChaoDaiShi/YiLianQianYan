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
| `RATE_LIMITED` | 429 | rate_limit | Yes |
| `PROVIDER_AUTH_FAILED` | 502 | permanent | No |
| `PROVIDER_UNAVAILABLE` | 502 | retryable | Yes |
| `SERVICE_UNAVAILABLE` | 503 | retryable | Yes |

Clients must branch on `code` and `retryable`, not localized message text.
