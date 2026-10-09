# Canvas UX Redesign / Stable Studio C2

Baseline: e98254c6c1fa08a475984d676bc3e20ea9031657.
Branch: feat/v1-canvas-studio-c2. Inline execution; no subagents.

## Diagnosis and choice

The existing page uses a PageHeader, warning row, always-on grouping row, Trail,
Canvas and Inspector. At 1366 the central surface is only about 737px wide.
The transparent shell illustration and repeated bordered panels compete with the
graph. Camera controls are split between upper-left, lower-left and minimap.
Inspector opens every section at once, regardless of the user's immediate task.

Options considered: restyle the three columns (insufficient space improvement);
permanent narrow rails (still consumes graph space); a full canvas with optional
floating panels (chosen). The user's reference establishes hierarchy, not colors
or additional publishing capability.

## Visual / interaction specification

- A roughly 60px studio header: back, graph identity, truthful save status,
  Trail / Properties / More toggles, and a disabled publishing placeholder.
- A solid, theme-aware quiet canvas covers the workspace illustration on this
  page only. Low contrast 24px dots, restrained purple accents, no glass blur.
- One lower-left floating strip: minus, actual zoom output, plus, Fit, 100%,
  explicit Auto Layout, Add node, Run selected node. No automatic camera commands.
- Optional Trail, Inspector and More panels float over the surface. Only one is
  visible at a time; they remain mounted so collapsing does not discard a draft
  or remount ReactFlow. Opening/closing panels never alters node coordinates or
  viewport. Selection does not automatically open panels.
- More holds existing grouping, review and refresh controls. Errors remain
  visible, with retry available; model availability belongs to relevant tools.
- All cards remain 240x128. A type icon, 15px title, 12px type/executor line,
  13px two-line summary and 12px status/attempt row have fixed information areas.
  Approval, human checkpoint, task, output and MCP-reserved treatments differ
  by icon and subtle semantic accent, never by content-driven dimensions.
- No new backend kinds: an existing work node without incoming edges may be
  labelled "入口任务" as a graph role. Output styling uses the existing projection
  role. MCP styling is presentation-only; not a new executable node kind.
- Edges use a thin smooth path and small arrow; selection strengthens the line.
- Run uses the same configured-executor/graph-lock checks and existing
  startTaskExecution action. It is explicitly a selected-node action, not a new
  graph scheduler. Publishing is disabled and labelled "未接入".
- Inspector retains every existing field and action. Execution/configuration
  sections are grouped with native details to avoid an always-expanded long form.
  An explicit extension section reserves capability, parameter and publishing
  slots, disabled and labelled as unavailable. No MCP discovery or business logic.

## Boundaries and implementation plan

1. [x] Capture the current real Canvas at 1280/1366/1920/2560, write geometry and
   screenshots to ignored target/canvas-studio. Add a reusable Edge scenario.
2. [x] Compose Studio header and optional panels in TaskWorldPage. Extract
   presentation-only header/toolbar/extension components. Preserve graph hooks,
   keyed provider lifetime and queue/reconcile/camera model implementations.
3. [x] Add node presentation mapping and focused semantic tests. Restyle cards,
   edges and scoped studio CSS. Add only save-status notifications at hook level;
   do not modify writer ordering, retry, rebase or persistence payloads.
4. [x] Adapt E2E navigation to explicitly open previously permanent panels; retain
   all existing Canvas Stability assertions. Add panel-toggle, real save-state,
   tool reachability, responsive and position/camera geometry assertions.
5. [x] Run focused checks while editing, then one final frontend test/build gate,
   architecture checks, real backend core + Canvas stability + Studio matrix.
6. [x] Inspect actual screenshots, record before/after metrics, changed files,
   limitations and evidence paths in the final report; stop after delivery.

No Backend/Cargo/REST/schema/migration/Secret/v2 changes. No global theme redesign,
new dependencies, arbitrary node resizing or bundle splitting. User explicitly
requested analysis, a concrete proposal and immediate implementation; no additional
design approval checkpoint is required. No push or merge is inferred for this new
task; small local commits are explicitly requested.

Acceptance and measured results: [C2 report](canvas-studio-c2-report.md).
