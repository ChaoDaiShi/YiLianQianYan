Status: completed

Commits:
- `73e1539` `feat(ui): apply cyrene foundation to shared components`

Test summary: `npm.cmd test -- src/theme/applyTheme.test.ts` failed first on `--radius-sm`; after implementation `npm.cmd test -- src/theme/applyTheme.test.ts src/theme/presets.test.ts`, `npm.cmd test`, `npm.cmd run build`, and `git diff --check` all passed.

Concerns:
- `npm.cmd run build` still reports the existing Vite chunk-size warning for `dist/assets/index-BdltOLFh.js` exceeding 500 kB after minification; this task did not change chunking.

Report path: `F:/项目开发/忆涟千言/YiLianQianYan/.worktrees/feat-v09-ui-foundation/.superpowers/sdd/task-3-report.md`

## Review Fix Evidence

- Files:
  - `frontend/src/theme/applyTheme.ts`
  - `frontend/src/theme/applyTheme.test.ts`
  - `frontend/src/components/ui/Modal.tsx`
  - `.superpowers/sdd/task-3-report.md`
- Semantic RED:
  - Added `derives non-cyrene semantic variables from the active preset palette` to `src/theme/applyTheme.test.ts`.
  - `npm.cmd test -- src/theme/applyTheme.test.ts` failed before the fix with `expected '#f9f7ff' to be '#f3f5f6'`, proving `buildThemeVariables()` was still emitting the Cyrene semantic palette for `PRESETS["precision-neutral"]`.
- Semantic GREEN:
  - Updated `buildThemeVariables()` to keep exact `CYRENE_SEMANTIC_TOKENS` only for `cyrene-ripple`, while deriving semantic surface/accent/sidebar/text/shadow variables from `theme.colors` for non-Cyrene and custom/imported themes.
  - `npm.cmd test -- src/theme/applyTheme.test.ts` passed after the change.
  - `npm.cmd test -- src/theme/applyTheme.test.ts src/theme/presets.test.ts` passed after the change.
- Modal fix:
  - Added protected dialog semantics to `Modal.tsx`: `role="dialog"`, `aria-modal="true"`, labelled title, backdrop marked `aria-hidden`, panel focus on open, Escape-to-close, click containment, and focus restoration to the previously focused element on close.
  - Kept the existing public props and Drawer behavior unchanged.
- Modal verification:
  - The current frontend test setup does not have an installed DOM runtime (`node_modules/jsdom` and `node_modules/happy-dom` were both absent), so no reliable automated dialog behavior test was added in this pass.
  - Static verification was completed by matching the Modal lifecycle against the existing Drawer pattern and by confirming the updated component compiles in the full `npm.cmd test` and `npm.cmd run build` passes.
- Covering verification:
  - `npm.cmd test -- src/theme/applyTheme.test.ts`
  - `npm.cmd test -- src/theme/applyTheme.test.ts src/theme/presets.test.ts`
  - `npm.cmd test`
  - `npm.cmd run build`
  - `git diff --check`
- Results:
  - `npm.cmd test` passed with `14` files and `89` tests green.
  - `npm.cmd run build` passed; Vite still emitted the existing chunk-size warning for `dist/assets/index-DXbenIaY.js` exceeding `500 kB` after minification.
  - `git diff --check` exited cleanly for whitespace errors; Git printed existing LF/CRLF conversion warnings for edited files in this Windows worktree.
- Remaining limitations:
  - Modal interaction coverage is still static/build-level only until a supported DOM test runtime is installed for this frontend workspace.
