# Settings UI Polish Implementation Plan

> **For agentic workers:** Execute inline in this session. Repository instructions prohibit unrequested subagents. The user explicitly prohibited tests, so steps omit TDD and test execution.

**Goal:** Rebuild the settings surface into six non-duplicated groups with one save action, consistent visual components, readable typography, and Microsoft-inspired responsive use of wide windows.

**Architecture:** Preserve `SettingsPage` as the state/API owner. Add pure navigation metadata and a presentational section frame, then compose existing settings sections into six groups. Keep responsive behavior in the existing stylesheet layers and reuse theme tokens/Lucide icons.

**Tech Stack:** React, TypeScript, Tailwind utility classes, existing CSS tokens, Lucide React, Vite.

## Global Constraints

- Base is `9f935304e394ccbbd10747fa2666f29167f15983` in an isolated `feat/v1-settings-ui-polish` worktree.
- Frontend settings UI only; no backend, Cargo, database, REST, Secret semantics, Canvas, Chat, Voice runtime, v2, dependency or schema changes.
- One save action only. No duplicate setting entry points or new features.
- Body/control text at least 14px; helper text at least 12px.
- Responsive widths: compact under 900px, desktop 900–1599px, wide at 1600px and above.
- No tests or E2E. Run only one final `npm run build` after all edits.

---

### Task 1: Navigation and Section Shell

**Files:**

- Create: `frontend/src/features/settings/model/settingsNavigation.ts`
- Create: `frontend/src/features/settings/components/SettingsSectionFrame.tsx`
- Modify: `frontend/src/features/settings/model/types.ts`
- Modify: `frontend/src/pages/SettingsPage.tsx`

**Interfaces:**

- `SettingsGroupKey = "model" | "voice" | "agent" | "security" | "extensions" | "appearance"`.
- `resolveSettingsGroup(value?: string | null): SettingsGroupKey` maps old deep links to their new group.
- `SETTINGS_GROUPS` supplies title, description, icon and accent class once.
- `SettingsSectionFrame` accepts group metadata and renders the page intro plus `.settings-card-grid`.

- [x] Replace the eight-row category list with six icon rows and descriptions.
- [x] Move save status and the only save button into `PageHeader.actions`.
- [x] Remove `saveActions` and stop passing a duplicate action into `ModelSettings`.
- [x] Compose Agent + Compaction, Permission + Sandbox, and Skills + Subagents in one group each.
- [x] Preserve legacy query-string links through `resolveSettingsGroup`.
- [x] Commit: `feat(settings): consolidate settings navigation and save actions`.

### Task 2: Unified Setting Cards

**Files:**

- Modify: `frontend/src/features/settings/sections/AgentSettings.tsx`
- Modify: `frontend/src/features/settings/sections/CompactionSettings.tsx`
- Modify: `frontend/src/features/settings/sections/PermissionSettings.tsx`
- Modify: `frontend/src/features/settings/sections/SandboxSettings.tsx`
- Modify: `frontend/src/features/settings/sections/SkillPathSettings.tsx`
- Modify: `frontend/src/features/settings/sections/VoiceSettings.tsx`
- Modify: `frontend/src/features/settings/sections/AppearanceSettings.tsx`
- Modify: `frontend/src/features/security/GrantEditor.tsx`

**Interfaces:**

- `.settings-card` is the shared surface for one logical setting group.
- `.settings-card--wide` spans the wide-screen grid for long text/path/security data.
- `.settings-section-heading`, `.settings-control-label`, `.settings-help` standardize hierarchy.

- [x] Wrap every logical unit in one shared card instead of ad-hoc borders and uppercase headings.
- [x] Keep common labels at 14px and helper text at 12–13px.
- [x] Put the current security grants before a collapsed “创建资源授权” disclosure.
- [x] Keep destructive controls visible and separate from ordinary options.
- [x] Preserve every existing callback, field, value, confirmation and API behavior.
- [x] Commit: `style(settings): unify cards typography and security hierarchy`.

### Task 3: Responsive Desktop and Wide-Screen Layout

**Files:**

- Modify: `frontend/src/styles/system.css`
- Modify: `frontend/src/styles/workbench.css`
- Modify: `frontend/src/styles/shell.css`

**Interfaces:**

- `.system-settings-layout`, `.system-settings-nav`, `.system-settings-content`, `.settings-form`, `.settings-card-grid` own all settings breakpoints.
- Existing semantic color and focus variables remain the source of truth.

- [x] Consolidate duplicate settings layout overrides into the settings stylesheet block.
- [x] Implement horizontal compact navigation below 900px.
- [x] Implement fixed left navigation and 960px content on regular desktop.
- [x] Implement 1280px content and two-column independent cards above 1600px.
- [x] Ensure text areas, secrets, paths and wide cards span both columns.
- [x] Add restrained brand glow and icon tile treatment without bitmap art or heavy glass.
- [x] Commit: `style(settings): add responsive Microsoft-inspired layout`.

### Task 4: Source Review and Production Build

**Files:**

- Review all files changed from base `9f935304e394ccbbd10747fa2666f29167f15983`.
- Create ignored evidence: `target/settings-ui-polish/result.json`.

**Verification:**

- No unit, integration, E2E or visual test commands.
- Read-only source audit: one save button implementation, six navigation groups, legacy aliases, minimum font classes, no protected-path changes.
- Run exactly once after all edits: `cd frontend && npm.cmd run build`.

- [x] Inspect the final diff for duplicate settings actions and protected files.
- [x] Run the single production build and retain its output.
- [x] Record build result, branch/HEAD, changed files, known limitations and tests not run.
- [x] Commit: `docs(settings): record UI polish scope and build result`.
