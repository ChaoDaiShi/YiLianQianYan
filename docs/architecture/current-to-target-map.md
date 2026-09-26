# Current → Target Map

> Where each part of mandate §4 (backend) and §17 (frontend) stands.
> Last updated at R7.

Status legend: **done** · **partial** · **not started** · **deviated** (with reason)

## Backend

| Target | Status | Where it is now |
|---|---|---|
| `app/{mod,bootstrap,state,router,lifecycle}.rs` | done | `app/` — R1. `server.rs` is a 13-line facade. |
| `core/{error,ids,event,command,resource,presence,time}.rs` | deviated | The kernel lives in `shared/` (event, command, resource, presence, voice, contracts). Renaming it to `core/` would be churn without a boundary change; `shared/` is the stable name in use. |
| `modules/settings/` | done | domain / application / api. R2. |
| `modules/task/` | partial | Relocated and HTTP-split (R3a/R3b). `application/` service extraction is **not started**; business logic still sits in the route modules (`create_graph` 71, `run_workflow_graph` 157, `dispatch_execution` 66 lines). |
| `modules/workflow/` | partial | Same as task: relocated and HTTP-split, `application/` not extracted. |
| `modules/capability/` | not started | `capability/` still at the root. |
| `modules/memory_skill/` | done | candidate / review / version / repository / evidence / sensitivity / validator / service / store. R5c. |
| `modules/voice/` | not started | `voice/` still at the root (1,195-line `runtime.rs`, 1,067-line `provider.rs`). |
| `modules/resource/` | done | limits / parsers / preview / ingest / binding / model. R5b. |
| `modules/artifact/` | not started | Artifact handling lives in `modules/task/artifact.rs`. |
| `modules/approval/` | not started | Approval lives in `modules/task/approval.rs` and `api/approvals.rs`. |
| `integrations/llm/` | not started | `llm/` still at the root. |
| `integrations/mcp/` | partial | Relocated under `integrations/mcp/` (R4) with facades. The mandated `protocol/ transport/ runtime/ registry/ result/ security/` subdivision is **not started**; `mcp_transport_config` still lives in `app/state.rs`. |
| `integrations/secret/` | not started | `secret/` still at the root. |
| `integrations/filesystem/` | not started | No such module; file access is spread across `db/`, `tools/` and `workspace/`. |
| `db/` | done (as-is) | Deliberately not moved. It owns schema and migrations, and no migration changed during this refactor. |
| `safety/` | done (as-is) | Deliberately not moved. Protected Kernel. |

### Estimate of what remains

The backend is roughly **half** migrated by module count. The unrelocated
root-level domains (`voice`, `capability`, `llm`, `secret`, `agent`, `tools`)
are the larger half and are the natural next slice. `modules/task/application`
and the MCP subdivision are the two places where an already-started module is
incomplete.

## Frontend

| Target | Status | Where it is now |
|---|---|---|
| `app/{App,router,providers,bootstrap}` | partial | `App.tsx` plus `surfaces/`. No `app/` directory. |
| `core/{api,events,commands,auth,types}` | deviated | Lives in `api/` and `types/`. `api/client.ts` is the shared HTTP core; `api/*` is already split per domain. |
| `features/task-world/` | partial | `canvas/`, `inspector/` (R6a) and `hooks/` (R6b) exist. `TaskWorldCanvas.tsx` is **not** split into `canvas/{TaskWorldCanvas,TaskNode,CanvasToolbar,viewport,layout}`; `model/` and `execution/` are not created (`taskGraphProjection.ts` and `TaskExecutionTrail.tsx` stay at the feature root). |
| `features/settings/` | not started | `pages/SettingsPage.tsx` (726 lines) is unmoved. |
| `features/voice/` | not started | `features/voice/GlobalVoiceHost.tsx` (895 lines) is unmoved; the other voice files are already feature-local. |
| `features/*` (conversation, capability, memory-skill, resource, artifact) | partial | `features/` exists with capabilities, execution, llm, mcp, memory, resources, security, skills, task-world, tasks, voice. Conversation still lives in `components/chat/`. |
| `surfaces/workspace/` | done | `surfaces/workspace/` exists; `surfaces/desktop/` holds the Tauri-specific surface. |
| `ui/{primitives,layout,feedback}` | deviated | Primitives live in `components/ui/`, layout in `components/layout/`. The boundary rule is enforced by name-independent path checks. |
| `styles/{tokens,base,shell,features/*}` | deviated | `index.css` (3,389 lines) is **not split**. See below. |

### Why `index.css` was not split

Mandate §22 asks for a feature-grouped split. That is not achievable here
without changing the cascade:

- The file's rules are **interleaved by feature**, not blocked by it:
  `.task-world-*` is at lines 146–210, `.conversation-*` at 222–370,
  `.capability-*` at 1028 and 2035, `.system-*` at 2977+. Grouping by feature
  means reordering rules within the same cascade layer.
- Tailwind v3 hoists `@layer` content to its `@tailwind` directives, and the
  unlayered rules must stay *after* that hoisted output. An `@import`-based
  split has to place imports **before** `@tailwind` (PostCSS requires it), which
  moves the unlayered rules ahead of the hoisted output. This was tested, not
  assumed: the built stylesheet changed (rule order moved, md5
  `5889431e` → `0153795f`), and it was reverted.

There is also **no visual regression test** in the suite — the CSS-facing tests
are source-text assertions, so a cascade mistake would ship silently. Given
§22's own "只移动现有规则" and the bar of "no visual redesign", the split is
deferred until a built-CSS diff or visual snapshot can guard it. The method is
ready: build, compare `dist/assets/index-*.css` md5 against the baseline
`5889431edc65281a20589a3953c724e2`. A provably order-preserving split is
possible if the unlayered blocks are moved to JS imports in `main.tsx` while
the `@layer` blocks stay in the Tailwind-processed file — that is the shape to
attempt next, not a feature-grouped one.
