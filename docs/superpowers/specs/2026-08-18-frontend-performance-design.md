# Phase 5A Frontend Bundle and Rendering Performance Design

## Goal

Reduce the initial frontend JavaScript loaded by YiLianQianYan while keeping the Phase 1–4B frozen UI, real data semantics, Agent behavior, security boundaries, and desktop runtime behavior unchanged.

## Baseline

- Branch: `develop`
- Baseline HEAD: `0571925f0a6061b4d68c42709f7154f87b7592bc`
- Main JavaScript: `597.48 kB` (`177.02 kB` gzip)
- CSS: `64.35 kB`
- Vite warning: one minified chunk larger than `500 kB`
- Build modules transformed: `1873`
- Current route loading: all page modules are synchronously imported by `src/App.tsx`

## Scope and invariants

This phase is limited to frontend route loading, import boundaries, measured render optimizations, tests, and build verification. It must not modify Backend API Schema, Database Schema, Agent Runtime, SSE Event Semantics, Memory or Embedding algorithms, MCP Protocol, Security Gateway, Approval Logic, Workflow Runtime, Task Runtime, or Tool Runtime.

The existing UI visual language, navigation, page behavior, accessibility, error handling, real-data display, and Tauri control-session bootstrap remain unchanged. No feature is removed and no security check is weakened.

## Architecture

`ChatPage` and its App Shell dependencies remain eager so the Workbench Home and active Conversation are immediately available. Low-frequency page boundaries use `React.lazy` and a shared `Suspense` fallback built from the existing `Skeleton` primitive. The route table remains in `App.tsx`; only page module loading changes.

The first implementation relies on Vite/Rollup's automatic dynamic-import chunking. `manualChunks` is not added unless the post-split build shows a clearly identified large dependency crossing a stable feature boundary. No component-level lazy loading is introduced.

Rendering changes are evidence-driven and local. Candidate checks include Markdown message rendering, Tool Card and Agent feedback rendering, execution rows, and stable derived list filtering. A candidate is changed only when its current dependency or render path demonstrates repeated work; broad `React.memo`, `useMemo`, or Store rewrites are out of scope.

## Route loading design

The eager path contains:

- `AppShell`
- `NavRail`
- `ChatPage`
- Workbench Home, Conversation, Composer, Message List, Agent feedback, and shared UI primitives used by that path

The lazy path contains:

- `TaskCenterPage`
- `SystemPage`
- `LogsPage`
- `SettingsPage`
- `SkillsPage`
- `PluginsPage`
- `WorkflowsPage`
- `WorkspacesPage` and `WorkspaceDetailPage`
- `AgentsPage`
- `CapabilitiesPage`
- `MemoryPage`
- `KnowledgePage`

Each lazy route must resolve through the existing route tree and render a compact loading surface without changing the URL, navigation labels, or page-level loading/error states. Dynamic imports must remain local Vite assets; no CDN or runtime network module loading is allowed.

## Rendering design

The first pass will inspect current selectors and component boundaries before changing them. Markdown remains the existing `react-markdown` plus `remark-gfm` chain. Streaming remains event-compatible and does not receive per-token animation or parser replacement. If a stable, expensive child is proven to rerender unnecessarily, the smallest local boundary is memoized and covered by a behavior-facing test.

Long lists are measured before any virtualization decision. If current list sizes and render paths do not show a concrete problem, no virtualization dependency is added. Workflow graph data, runtime APIs, and editor UX remain untouched.

## Testing and verification

Add focused frontend tests for:

- route loader classification and eager core preservation;
- lazy route module presence and Suspense fallback contract;
- key route rendering compatibility without changing route paths;
- any actual rendering optimization introduced.

Run the full `npm.cmd test -- --run`, `npm.cmd run build`, and `git diff --check`. Compare initial/main chunks, route chunks, total bundle, CSS, and gzip before/after. Start the Tauri runtime and verify Home, Conversation, Workflow, Memory, Settings, and as many remaining lazy routes as practical, including repeat navigation and absence of chunk-load errors. Report any route not directly exercised.

## Explicit non-goals

- No `manualChunks` by default.
- No framework, router, state library, Markdown engine, or Workflow Editor migration.
- No backend, Rust, API, SSE, database, security, approval, or runtime changes.
- No UI redesign, theme change, navigation reorganization, or new feature.
- No Phase 5B, Phase 5C, release gate, voice, or Live2D work.
