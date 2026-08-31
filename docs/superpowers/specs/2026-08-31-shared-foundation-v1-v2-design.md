# AI-0 Shared Foundation & Integration Spine Design

## Objective

Prepare `main@7906ce7d8cbc29b207a22a222f24c52f594eae35` for independent v1 Task World and v2 Desktop World development. The deliverable is a thin, executable integration spine. It preserves every v0.9 business boundary and stops before implementing either product line.

## Chosen approach

Use an incremental in-repository spine:

- keep React business access on HTTP/SSE and keep Tauri IPC limited to native host concerns;
- add a focused Rust `shared` module for contracts and small in-memory services;
- expose adapters through new REST/SSE endpoints without rewriting Agent, Workflow, MCP, Memory, Workspace, Approval, Capability, or the Security Execution Gateway;
- make `frontend/src/api/client.ts` a compatibility facade and expose domain entrypoints for all future imports;
- extract the current route shell as `WorkspaceSurface`, with a transparent `StandaloneHost` and a no-product-feature `DesktopSurfaceSkeleton`;
- store ingested files under an application-managed resource directory and persist only resource identity/metadata in SQLite;
- provide explicitly simulated cross-domain commands and projections so neither downstream line waits for the other.

Two alternatives were rejected. A new shared Rust crate plus generated TypeScript schema would over-partition the current two-package workspace and add release tooling before contracts stabilize. A contracts-only documentation layer would be smaller, but it would fail the mandatory functional Event, Command, Resource, Voice, and Gate 0 chains.

## Non-negotiable boundaries

- `Workspace` remains a project/task container; no `DesktopSpace` variant is introduced.
- Workflow remains an execution DAG; no TaskGraph or canvas layout fields are added.
- Capability Registry remains discovery-only and receives no execute/invoke method.
- Command routing does not authorize side effects. Real side-effect handlers must call the existing Security Execution Gateway. AI-0 mock desktop handlers have no real side effect and return `simulated: true` and `provider: "mock"`.
- Resource ingestion grants no execution permission and never runs or parses uploaded content.
- Product events alone use the shared event spine. Chat tokens, audio media, desktop telemetry, Windows messages, and monitor samples remain on dedicated paths.
- External web or plugin UI must not share the trusted high-privilege main WebView by default. AI-0 documents the future restricted-window boundary without implementing sandbox WebViews.
- Contract evolution is additive within a schema version. Breaking changes require incrementing `schema_version`.

## Architecture

### Database migration foundation

`Database::new` first determines whether a recognizable v0.9 schema already exists, then executes the unchanged idempotent v0.9 bootstrap, creates `schema_migrations`, records migration `0000` as the adopted baseline, and applies ordered Shared/Core migrations in the `0000-0999` namespace. Migration application is transactional and records a version only after its SQL succeeds.

The initial incremental migration creates `resources`. No old table is dropped or recreated. Opening a fresh database, adopting an existing v0.9 database, and opening the same database repeatedly are contract tests.

### Shared Rust boundary

`backend/src/shared/` contains small units:

- `contracts.rs`: common schema version, source, error, and simulation metadata;
- `event.rs`: `YiEvent`, `EventHub`, and namespace validation;
- `command.rs`: `CommandRequest`, `CommandResult`, `CommandRouter`, validation, and handler registration;
- `resource.rs`: safe resource identity, metadata, name normalization, hashing, and storage service;
- `voice.rs`: provider-independent session state machine, deterministic STT/TTS providers, interrupt/cancel semantics, layered presence, narration fallback, and attention policy;
- `context.rs`: bounded `ContextProvider` contract plus Task and Desktop projections and deterministic mock providers.

These modules share protocols and small infrastructure, not Task or Desktop state machines.

### Event spine

`EventHub` uses Tokio broadcast for bounded, ephemeral product events. `/api/events` emits JSON SSE records and treats lag as a recoverable stream error event rather than panicking. `resource.created` proves the complete backend emission path. The frontend event client accepts unknown additive payload fields.

### Command spine

`CommandRouter` validates the command namespace, request ID, and source, resolves one registered handler, and returns a structured result for success, validation failure, missing handler, or handler failure. It does not grant permission. Initial handlers are `core.echo`, `presence.get`, `desktop.app.open`, and `desktop.space.switch`; the two desktop handlers are L1 mocks only.

`POST /api/commands` is protected by the existing control-session middleware. Future handlers with real side effects must adapt through the Security Execution Gateway before execution.

### Resource core

The browser sends selected file bytes to `POST /api/resources/ingest` with a percent-encoded name and media type. The backend:

1. validates a bounded non-empty body and safe display name;
2. generates a resource ID;
3. hashes bytes with SHA-256;
4. writes to an application-managed resource root using a create-new temporary file followed by an atomic rename;
5. stores the final managed path and metadata in SQLite;
6. emits `resource.created`;
7. returns the persisted resource.

`GET /api/resources` and `GET /api/resources/:id` expose metadata, not raw filesystem access. Original absolute paths are never persisted as task references.

### Voice, presence, and narration

Voice is a provider-neutral in-memory foundation. A `VoiceSession` supports start, stop, interrupt, and cancel with explicit invalid-transition errors. Deterministic providers make transcript and speech output testable without cloud credentials. Voice emits product events, while audio bytes never enter `EventHub`.

Presence is a snapshot with independent activity, interaction, and attention dimensions. Narration uses a bounded deterministic template fallback and `silent`, `balanced`, or `companion` attention policy. Voice never calls task or desktop operations directly.

### Context and projections

`ContextProvider` receives a bounded request/scope object and returns a small typed context fragment. AI-0 provides only deterministic mocks. `TaskProjection` exposes summary fields without TaskGraph. `DesktopContextProjection` exposes an optional space/app/window summary and capability IDs without HWND or window-tree internals.

### Frontend boundary and surfaces

The existing API implementation moves unchanged to `legacy.ts`. `client.ts` becomes a compatibility facade. Domain modules (`transport`, `chat`, `conversations`, `tasks`, `workflows`, `workspaces`, `capabilities`, `memory`, `plugins`, `system`, `events`, `commands`, and `resources`) give future code non-conflicting entrypoints. Existing imports continue compiling.

`WorkspaceSurface` owns the current route tree and `AppShell`. `StandaloneHost` renders it unchanged. `DesktopSurfaceSkeleton` renders the same `WorkspaceSurface` behind an explicit host boundary and adds no visual product feature. `AppRoot` selects a host through a typed host mode whose default is standalone.

### Error and compatibility semantics

- API errors use stable machine-readable codes and safe user-facing messages.
- Missing commands return a structured `not_found` result, not an HTTP crash.
- Invalid resources are rejected before durable metadata is written; partial files are removed.
- Unknown JSON fields deserialize successfully by default; all contract tests include additive fields.
- Unknown frontend product event namespaces are delivered to the subscriber unchanged.
- Mock results are never labeled as real execution success.

## Testing and acceptance

TDD covers every new state transition and contract. Gate 0 requires:

- fresh, v0.9-upgrade, and idempotent migration tests;
- standalone and desktop-skeleton surface contract tests;
- backend EventHub to SSE serialization plus frontend event consumption;
- frontend command request to backend router structured mock result;
- resource byte ingest to managed storage, hash, database query, and event;
- voice lifecycle, deterministic STT/TTS, interrupt, presence, and narration tests;
- mock TaskProjection, DesktopContextProjection, and desktop command tests;
- full Rust tests, frontend tests, production build, Tauri check, live backend smoke, and the existing Windows GUI startup smoke where the environment permits it.

## Rollback and stopping rule

Each phase is independently committed. Migrations are forward-only; rollback never deletes user data. If the Surface commit causes a visible regression it is reverted before later work. AI-0 stops after Gate 0, CI, docs, both handoffs, clean status, immutable foundation tag, and branch push. TaskGraph, Infinite Canvas, Task Supervisor, DesktopSpace, App Mount, wallpaper, widgets, desktop pet, complete voice product, and high-level parsers remain unimplemented.
