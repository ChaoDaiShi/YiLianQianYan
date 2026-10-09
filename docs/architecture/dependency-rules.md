# Dependency Rules

> Scope: the `refactor/v1-architecture-foundation` line. These rules describe the
> modular monolith this repository is converging on — not microservices, not
> separate deployables, not a second database.

## Backend

```text
Transport / UI  (api, Tauri command layer)
      ↓
Application     (modules/*/application, app/*)
      ↓
Domain          (modules/*/domain)
      ↓
Ports           (traits the domain/application declare)
      ↑
Infrastructure  (db, integrations, secret, filesystem)
```

1. **Domain must not know how it is transported.** `modules/*/domain` may not
   name `axum`, `crate::api`, `crate::app`, or `tauri`.
2. **API handlers parse, delegate and map.** A handler may parse a request,
   establish context, call an application service and map the response. It may
   not own a state machine, a provider protocol, or a domain policy.
3. **Infrastructure implements; it does not decide.** `db` and `integrations`
   may implement what the domain or application declares. Policy stays above
   them.
4. **A module reaches another module only through its public surface** —
   `modules/<name>/mod.rs` and the re-exports it declares — never through a
   sibling's internal file path.
5. **v1 must not reach into v2.** `modules::v2`, `crate::v2` and `V2TaskWorld`
   must not appear in v1 source, so a future merge cannot silently couple the
   two lines.
6. **Prefer structured interfaces over pixel automation** (project-wide rule,
   restated here because it is an architecture constraint, not a preference).

## Frontend

```text
App
 ↓
Feature Pages        (features/*/pages, pages/*)
 ↓
Feature Hooks/Model  (features/*/hooks, features/*/model)
 ↓
Feature API          (features/*/api.ts, api/*)
 ↓
Core HTTP/Event      (api/client.ts, api/events.ts)
```

1. **UI primitives are dumb.** `components/ui/**` may not import from
   `features/`, `pages/` or `stores/`.
2. **Features are independent.** `features/A` may not import `features/B`.
   Cross-feature work goes through a public feature facade or a shared
   contract in `core/`.
3. **Tauri is reached dynamically.** Outside `surfaces/desktop/`, a static
   `import ... from "@tauri-apps/…"` is forbidden — the dynamic form is what
   keeps the surface portable to a future web or cloud host.
4. **Business logic must not bind to Tauri** (the v1.1 constraint). The desktop
   shell may call business logic; business logic may not call the shell.

## Enforcement

These are checked, not merely documented.

| Check | Where | Covers |
|---|---|---|
| `frontend/src/architecture/boundaries.test.ts` | `npm test` | frontend rules 1–3, line budget |
| `backend/tests/architecture_boundaries.rs` | `cargo test` | backend rules 1, 5, line budget |

Both are **source-text** checks. They enforce direction, not types, and they are
deliberately lightweight — no new dependency was added to run them. A check
that reads source can be fooled by an alias or a re-export; treat a passing run
as necessary, not sufficient.

### Grading

- **Error** — a rule that holds today and must keep holding. These four do:
  no `components/ui` → feature import, no cross-feature import, no static Tauri
  import outside the desktop surface, no transport in a domain layer.
- **Error** — a *new* file above 600 lines. The files already above the budget
  are recorded by path in both checks; that list is recorded debt, not approval.
  A file leaves the list by being split. Nothing joins it.

Files legitimately above 600 lines today (38 backend, 6 frontend) are the
largest remaining refactor targets; they are listed in the final report.
