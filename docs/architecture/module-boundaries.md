# Module Boundaries

> What each boundary is for, and what is allowed to cross it.
> Rules: [dependency-rules.md](dependency-rules.md). Ownership:
> [ownership.md](ownership.md).

## The shape

One deployable. One process. One SQLite file. The boundaries below are
**compile-time and conceptual**, not network boundaries — the point is that a
change inside one domain does not require reading the others.

## Backend

| Layer | Path | Crosses inward | May reach outward to |
|---|---|---|---|
| Transport | `api/`, `app/router.rs` | HTTP requests, SSE | application services |
| Composition | `app/` | process start, config | everything (it is the wiring) |
| Application | `modules/*/application`, `modules/*/api` | use cases | its own domain, ports |
| Domain | `modules/*/domain` | — | nothing outside itself |
| Infrastructure | `db/`, `integrations/`, `secret/`, `isolation/` | — | the ports it implements |
| Shared contracts | `shared/` | — | nothing (stable kernel) |

`shared/` carries the **Protected Kernel**: event envelope, command envelope,
resource identity, presence, voice contracts. Code may move through these;
their semantics may not change silently. A change here is a
`SHARED_CONTRACT_CHANGE_REQUIRED` stop, not a refactor.

`safety/` is likewise a Protected Kernel — `SecurityExecutionGateway` and the
approval path. The security chain
`Workflow → SecurityExecutionGateway → Approval → Execution → Verification`
must survive any refactor intact.

## The module convention

Since R3a the project uses **one** shape:

```text
backend/src/modules/<domain>/
├── mod.rs          public surface (re-exports only)
├── domain/         types and policy; no transport
├── application/    use cases
├── api/            HTTP handlers
└── infrastructure/ adapters
```

Not every module has every layer, and a layer is created only when there is
real content for it — an empty `dto.rs` is a speculator, not a boundary.

Modules today: `settings`, `task`, `workflow`, `resource`, `memory_skill`.

**Still at the repository root**, and inconsistent with the convention:
`voice/`, `capability/`, `safety/`, `db/`, `agent/`, `tools/`, `skills`-related
files. R5 relocated `resource` and `memory_skill`; R3 relocated `task` and
`workflow`; R2 created `settings`. The remainder is deliberate, unstarted work,
recorded in the final report rather than half-moved here.

## Cross-domain interaction

Domains do not call into each other's internals. They interact through:

- **Commands** — an intent, routed (`shared/command.rs`)
- **Events** — an envelope broadcast on the hub (`shared/event.rs`)
- **Projections** — a read-only view for a surface (`modules/*/projection.rs`)
- **Application services** — the public entrypoint of a module

`modules/resource/binding.rs` currently reads
`modules::task::execution::MAX_NODE_CONTEXT_ITEM_CHARS`. That is a real
resource → task compile-time edge, found while splitting in R5 and deliberately
left rather than silently redesigned. It is the one cross-domain edge the
checks do not yet flag; R7's rule set covers domain→transport, not
domain→domain constants.

## Frontend

| Layer | Path | Rule |
|---|---|---|
| App | `src/App.tsx`, `src/surfaces/` | composes surfaces |
| Feature page | `features/*/pages`, `pages/*` | composes a feature |
| Feature hooks/model | `features/*/hooks`, `features/*/model` | behaviour and projection |
| Feature API | `features/*/api.ts`, `api/*` | transport per domain |
| Core client | `api/client.ts`, `api/events.ts` | shared HTTP and SSE |

`components/ui/**` is a primitive layer: it renders, it does not know a domain.

### The `?raw` contract tests

This codebase pins product behaviour with source-text assertions
(`import x from "./Foo.tsx?raw"`). They are strong where they are specific and
**brittle under a file split** — moving a string to another file makes the
assertion fail even though behaviour is unchanged. When splitting, migrate the
test by asserting across the new composition (page shell plus its hooks, or a
component plus its sections) so the union is still required to contain every
pinned string. Never delete an assertion to make a move compile.
