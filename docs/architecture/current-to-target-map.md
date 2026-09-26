# Current → Target Map

> Where each part of mandate §4 (backend) and §17 (frontend) stands.
> Last updated at R9d.

Status legend: **done** · **partial** · **not started** · **deviated** (with reason)

## Backend

| Target | Status | Where it is now |
|---|---|---|
| `app/{mod,bootstrap,state,router,lifecycle}.rs` | done | `app/` — R1. `server.rs` is a 13-line facade. |
| `core/{error,ids,event,command,resource,presence,time}.rs` | deviated | The kernel lives in `shared/` (event, command, resource, presence, voice, contracts). Renaming it to `core/` would be churn without a boundary change; `shared/` is the stable name in use. |
| `modules/settings/` | done | domain / application / api. R2. |
| `modules/task/` | partial | Relocated and HTTP-split (R3a/R3b). `application/` service extraction is **not started**; business logic still sits in the route modules (`create_graph` 71, `run_workflow_graph` 157, `dispatch_execution` 66 lines). |
| `modules/workflow/` | partial | Same as task: relocated and HTTP-split, `application/` not extracted. |
| `modules/capability/` | done | `modules/capability/` — R9b. `capability.rs` is a compatibility facade. |
| `modules/memory_skill/` | done | candidate / review / version / repository / evidence / sensitivity / validator / service / store. R5c. |
| `modules/voice/` | done | `modules/voice/` — R9a. `voice.rs` is a compatibility facade; the 1,195-line `runtime.rs` and 1,067-line `provider.rs` moved unchanged. |
| `modules/resource/` | done | limits / parsers / preview / ingest / binding / model. R5b. |
| `modules/artifact/` | not started | Artifact handling lives in `modules/task/artifact.rs`. |
| `modules/approval/` | not started | Approval lives in `modules/task/approval.rs` and `api/approvals.rs`. |
| `integrations/llm/` | done | `integrations/llm/` — R9c. `llm.rs` is a compatibility facade. |
| `integrations/mcp/` | partial | Relocated under `integrations/mcp/` (R4) with facades. The mandated `protocol/ transport/ runtime/ registry/ result/ security/` subdivision is **not started**; `mcp_transport_config` still lives in `app/state.rs`. |
| `integrations/secret/` | done | `integrations/secret/` — R9d. `secret.rs` is a compatibility facade. |
| `integrations/filesystem/` | not started | No such module; file access is spread across `db/`, `tools/` and `workspace/`. |
| `db/` | done (as-is) | Deliberately not moved. It owns schema and migrations, and no migration changed during this refactor. |
| `safety/` | done (as-is) | Deliberately not moved. Protected Kernel. |

### Estimate of what remains

The product domains are relocated. `modules/` holds capability, memory_skill,
resource, settings, task, voice and workflow; `integrations/` holds llm, mcp
and secret. Every domain the target table names now has a home, so the
remaining root-level directories are the ones the table does **not** assign a
module target: the agent runtime (`agent`, `tools`), the transport layer
(`api`, `app`) and the kernel/infrastructure the mandate leaves in place
(`shared`, `db`, `safety`, `config`, `execution`, `interaction`, `isolation`,
`plugin`, `utils`, `workspace`).

What is left inside started modules: `modules/task/application` and the MCP
subdivision (`protocol/ transport/ runtime/ registry/ result/ security/`), which
is also where `mcp_transport_config` still living in `app/state.rs` belongs.

## Frontend

| Target | Status | Where it is now |
|---|---|---|
| `app/{App,router,providers,bootstrap}` | partial | `App.tsx` plus `surfaces/`. No `app/` directory. |
| `core/{api,events,commands,auth,types}` | deviated | Lives in `api/` and `types/`. `api/client.ts` is the shared HTTP core; `api/*` is already split per domain. |
| `features/task-world/` | partial | `canvas/`, `inspector/` (R6a) and `hooks/` (R6b) exist. `TaskWorldCanvas.tsx` is **not** split into `canvas/{TaskWorldCanvas,TaskNode,CanvasToolbar,viewport,layout}`; `model/` and `execution/` are not created (`taskGraphProjection.ts` and `TaskExecutionTrail.tsx` stay at the feature root). |
| `features/settings/` | not started | `pages/SettingsPage.tsx` (726 lines) is unmoved. |
| `features/voice/` | partial | `features/voice/GlobalVoiceHost.tsx` (616 lines) is the host; its orchestrator concurrency (barge-in and final-transcript/continuation) is extracted into `bargeIn.ts` and `turnFlow.ts` as pure runtime-injected functions (R6g/R6h), covered by 18 interaction tests. The remaining host body is React glue and stays on the over-600 baseline. |
| `features/*` (conversation, capability, memory-skill, resource, artifact) | partial | `features/` exists with capabilities, execution, llm, mcp, memory, resources, security, skills, task-world, tasks, voice. Conversation still lives in `components/chat/`. |
| `surfaces/workspace/` | done | `surfaces/workspace/` exists; `surfaces/desktop/` holds the Tauri-specific surface. |
| `ui/{primitives,layout,feedback}` | deviated | Primitives live in `components/ui/`, layout in `components/layout/`. The boundary rule is enforced by name-independent path checks. |
| `styles/{tokens,base,shell,features/*}` | done, different shape | `index.css` is a 28-line manifest holding no rules; the rules live in ten files under `src/styles/`, split **by cascade layer** rather than by feature. The built stylesheet is byte-identical. See below. |

### How `index.css` was split, and why not by feature

§22 asks for a feature-grouped split. That is **not available** for this file:
its rules are interleaved by feature across cascade layers (`.task-world-*` at
146–210, `.conversation-*` at 222–370, `.capability-*` at 1028 and 2035,
`.system-*` at 2977+), so grouping by feature means reordering rules within a
layer.

The split is by **cascade layer instead**, which is order-preserving:

| File | Holds |
|---|---|
| `index.css` | the manifest — `@import` only, no rules |
| `tailwind.css` | the three `@tailwind` injections |
| `base.css` | `@layer base` — tokens, theme, resets |
| `workspace.css` | `@layer components` — shell, nav, feature pages |
| `capability.css`, `system.css` | `@layer components` |
| `utilities.css` | `@layer utilities` |
| `shell.css`, `responsive.css`, `animations.css`, `markdown.css` | **unlayered** |

`index.css` documents the two rules that make it work: `@layer` files are
hoisted to the injections so their position is free but their relative order is
not, and the unlayered files must come last because unlayered rules win over
every layer. **Do not rearrange that import list.**

How it is proved: build and compare `dist/assets/index-*.css` against the
baseline md5 `5889431edc65281a20589a3953c724e2`. That check earned its keep on
the first attempt — extracting by assumed line ranges folded three *unlayered*
`@media`/`@keyframes` regions into layer files, moving 45 rules. Enumerating the
top-level blocks by brace depth instead of guessing found all five unlayered
regions.

CSS is **not** covered by the automated boundary check (it reads `.ts`/`.tsx`
only), so a future CSS edit has no automated guard. The structural safeguard is
that `index.css` holds no rules, so it cannot sprawl — new CSS must go into a
named file under `styles/`.
