Status: completed

Commits:
- `b5ea0a8` `feat(ui): group navigation for cyrene theme`

Test summary: `npm.cmd test -- src/components/layout/navGroups.test.ts` failed first because `src/components/layout/navGroups.ts` did not exist. After implementation, `npm.cmd test -- src/components/layout/navGroups.test.ts`, `npm.cmd test`, `npm.cmd run build`, and `git diff --check` passed.

Concerns:
- `npm.cmd run build` still reports the existing Vite chunk-size warning for `dist/assets/index-D7U6HOXy.js` exceeding 500 kB after minification; this task did not change bundling strategy.
- `git diff --check` exited successfully but Git printed the existing Windows LF/CRLF normalization warning for `frontend/src/components/layout/NavRail.tsx`.

Report path: `F:/项目开发/忆涟千言/YiLianQianYan/.worktrees/feat-v09-ui-foundation/.superpowers/sdd/task-4-report.md`

## Scope

- Worked only in `frontend/src/components/layout/NavRail.tsx`, `frontend/src/components/layout/navGroups.ts`, `frontend/src/components/layout/navGroups.test.ts`, and this task report.
- Preserved all currently registered routes from `frontend/src/App.tsx`: `/chat`, `/workflows`, `/workspaces`, `/skills`, `/plugins`, `/agents`, `/capabilities`, `/knowledge`, `/system`, `/logs`, `/settings`.
- Did not add `/tasks`, `/memory`, or any other unregistered route.
- Did not modify backend, API, SSE, page logic, health polling behavior, or unrelated components.

## TDD Evidence

- RED:
  - Added `frontend/src/components/layout/navGroups.test.ts`.
  - Ran `npm.cmd test -- src/components/layout/navGroups.test.ts`.
  - Result: failed with `Failed to load url ./navGroups ... Does the file exist?`, proving the grouped navigation model did not exist yet.
- GREEN:
  - Added `frontend/src/components/layout/navGroups.ts` as a pure grouped model with three visual groups:
    - `核心`: `/chat`, `/workflows`, `/workspaces`
    - `能力`: `/skills`, `/plugins`, `/agents`, `/capabilities`, `/knowledge`
    - `系统`: `/system`, `/logs`, `/settings`
  - Updated `frontend/src/components/layout/NavRail.tsx` to render `NAV_GROUPS` instead of the old inline flat array.
  - Re-ran `npm.cmd test -- src/components/layout/navGroups.test.ts`.
  - Result: passed.

## Implementation Notes

- Extracted the icon-bearing navigation model into `navGroups.ts` so grouping is declarative and testable without rendering React.
- Preserved each item’s existing `to`, `id`, `label`, and icon component.
- Kept the current `NavLink`-based navigation semantics and active-route behavior.
- Kept backend health polling exactly as-is, including the same interval and health-state mapping.
- Added subdued visual group labels and separators in `NavRail` only.
- Switched active styling to `--accent-primary` and `--sidebar-active` with a fallback to `--accent-soft` so the nav can consume the requested token without broad theme-file changes in this task.
- Preserved the active left highlight marker.

## Verification

- Focused RED:
  - `npm.cmd test -- src/components/layout/navGroups.test.ts`
  - Failed because `navGroups.ts` was missing.
- Focused GREEN:
  - `npm.cmd test -- src/components/layout/navGroups.test.ts`
  - Passed: `1` file, `1` test.
- Full frontend tests:
  - `npm.cmd test`
  - Passed: `15` files, `90` tests.
- Production build:
  - `npm.cmd run build`
  - Passed with the existing Vite chunk-size warning only.
- Self-review:
  - `git diff -- frontend/src/components/layout/NavRail.tsx frontend/src/components/layout/navGroups.ts frontend/src/components/layout/navGroups.test.ts`
  - Confirmed the diff is limited to grouped nav extraction/rendering plus its new test.
  - `git diff --check`
  - Exited successfully; only the existing LF/CRLF warning was printed by Git.

## Self-Review Findings

- Verified the grouped model order matches the task brief exactly.
- Verified no route, label, or icon was removed or renamed.
- Verified `App.tsx` still contains no `/tasks` or `/memory` registration, and none were introduced by this task.
- Verified the only compile issue introduced during implementation was an unused `MessageSquare` import in `NavRail.tsx`; removed it and reran focused test, full tests, and build successfully.
