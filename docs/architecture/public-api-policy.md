# Public API Policy

> What YiLianQianYan promises to keep stable, and what it does not.
> Applies to the `refactor/v1-architecture-semantic` line and forward.

## The rule in one line

**Rust `pub` visibility is not a support commitment.** A path that a module
makes visible exists so *this workspace* can compile against it, not because a
third party may depend on it.

This matters because the R2 facade drain removed twelve module paths that were
`pub` (`yilian_backend::task::*`, `::server::*`, `::secret::*`, and nine more).
That was an intentional narrowing, not an oversight, and this document records
why it is not a compatibility break.

## Three layers

### A. Product Public Contracts — the supported API

These are the interfaces the product commits to. They are versioned with the
release, they are what a v1.1 surface or an external integration may rely on,
and a change to one goes through the release-notes and migration-note process.

| Contract | Shape | Where it is defined |
|---|---|---|
| REST | HTTP method + path + request/response body | `app/router.rs` route table, `docs/architecture/surface-boundary.md` |
| Command | command envelope | `shared/command`, `docs/architecture/event-command-contract.md` |
| Event | event envelope + SSE event types | `shared/event`, `docs/architecture/event-command-contract.md` |
| Projection | read models the UI consumes | `modules/*/projection`, `docs/architecture/event-command-contract.md` |
| MCP | protocol era, transport, catalogue | `integrations/mcp`, `docs/architecture/compatibility-facades.md` |
| Skill | `SKILL.md` layout and lifecycle | `modules/memory_skill` |
| Capability | capability descriptor and registry | `modules/capability` |
| Resource | resource identity and bounded ingest | `shared/resource`, `modules/resource` |
| Artifact | artifact identity and materialisation | `modules/task/artifact` |

The REST row is the one with the sharpest guarantee: the route-path set is
**frozen**. R1 and R2 both held it at 178 handlers with no method, path or
handler-signature change. Every refactor in this repository must preserve it.

Event and Command envelopes are the **Protected Kernel**
(`shared/`): code may move through them; their semantics may not change
silently. A change there is a `SHARED_CONTRACT_CHANGE_REQUIRED` stop.

### B. Workspace Internal Host API — explicit, unstable

The minimum surface another crate in *this workspace* links against:

```rust
yilian_backend::safety::ControlSession
yilian_backend::serve_in_background_with_control_token
```

Consumed by `src-tauri` (crate `yi-lian-qian-yan`). These are **workspace
internal**: no third-party stability promise, no semver-path guarantee. They are
listed here so that the set stays *explicit* — a new crate-root export for an
external consumer is a change to this table, not a casual `pub use`.

`backend/src/lib.rs` also re-exports `create_server`, `serve`,
`serve_in_background` and `AppServer` for the standalone `yilian-server` binary
and for the host. Those are part of this layer by the same reasoning.

### C. Internal Implementation — no promise at all

Everything under `modules/*`, `integrations/*`, `app/*`, `api/*`, `db/`,
`tools/`, `agent/`, `shared/*` internals. These paths may be renamed, split,
merged or deleted in any release. No migration note is owed.

If you are outside this workspace and you are importing one of these: that is
the mistake, not the removal.

## `publish = false`

`backend/Cargo.toml` sets `publish = false`. Reasons:

- No script, workflow or documented step in this repository runs `cargo publish`
  or `cargo package`; the release process builds a Windows/macOS desktop
  bundle.
- No third party consumes `yilian-backend` as a dependency.
- The crate version tracks the product release (`1.0.0-rc.2`) rather than a
  library semver, which is the wrong scheme for a published crate.

`publish = false` makes the layer-C status mechanical: a crate that is not
published cannot attract a dependent who mistakes a `pub` path for a contract.
If a publish requirement ever appears, removing that line is a deliberate
decision that also requires the layer-A/layer-B tables above to be revisited.

## If a Rust SDK is ever needed

Do **not** restore the legacy facades, and do not widen this crate's surface
back to "everything that used to be `pub`". Plan a separate `yilian-sdk` crate
that:

1. depends on `yilian-backend` (which stays `publish = false`);
2. exposes only deliberately designed, versioned contracts — the layer-A shapes
   plus whatever a client genuinely needs;
3. carries its own semver and its own deprecation policy.

A SDK is a product with a support cost. It should be started because a consumer
exists, not because a path used to be public.

## Related documents

- [compatibility-facades.md](compatibility-facades.md) — what was removed, and
  the old → owner table.
- [dependency-rules.md](dependency-rules.md) — direction of dependencies.
- [event-command-contract.md](event-command-contract.md) — the Protected Kernel.
- [surface-boundary.md](surface-boundary.md) — the REST surface.
- `docs/refactor/r2-final-report.md` — the change that narrowed the surface.
