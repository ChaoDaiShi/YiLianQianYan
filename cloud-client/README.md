# YiLian Cloud Client

The public cloud client is implemented inside the local Rust backend so browser code never stores cloud refresh tokens or provider credentials. It consumes only `cloud-contract/v1/openapi.yaml`, persists redacted session and sync metadata in local SQLite, and resolves sensitive values through the existing SecretStore abstraction.

The client is optional. Missing configuration, network failure or authentication expiry must degrade to local-only behavior. It does not contain Admin routes, private database models or private deployment logic.
