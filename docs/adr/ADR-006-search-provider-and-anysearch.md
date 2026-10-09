# ADR 006 Search Provider and AnySearch

状态：Accepted
日期：2026-10-06

## Decision

AI Search depends on a provider-neutral `SearchProvider` interface with search, extraction and capability discovery. AnySearch is the first adapter. Direct search uses `POST https://api.anysearch.com/v1/search`; anonymous requests omit Authorization, while BYOK requests resolve a local SecretStore entry and send a Bearer header.

The adapter preserves the provider request ID and maps HTTP and business failures into stable local categories. It never logs Authorization or credential-bearing response messages. A YiLian Cloud Search Proxy may implement the same public contract without exposing the platform provider key to the client.

## Consequences

- UI and Agent capabilities consume normalized results instead of AnySearch-specific response objects.
- Provider replacement does not require rewriting Search UI actions.
- Search results can flow into existing Chat, Task, Resource and Knowledge features through their public APIs.
