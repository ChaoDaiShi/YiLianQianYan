# ADR 003 Secret Storage Policy

状态：Accepted
日期：2026-10-06

## Decision

LLM, STT, TTS, AnySearch and MCP credentials remain local-first and are stored through the existing OS SecretStore abstraction. Ordinary settings, sync payloads, logs, metrics, support bundles and public API responses never contain secret plaintext.

Cloud synchronization may carry provider name, model, non-sensitive base URL and `credential_state` values such as `local_only` or `required_on_device`. Secret references are local implementation details and are not portable credentials. A future Cloud Secret Vault requires a separate ADR and is outside v1.1.

## Consequences

- Login or sync enablement never uploads existing keys by default.
- Logout deletes session credentials by default and deletes provider keys only after explicit selection.
- Search 402 responses that contain generated credentials are treated as sensitive and are never written to routine logs.
