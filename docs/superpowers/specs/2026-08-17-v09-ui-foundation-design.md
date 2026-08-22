# YiLianQianYan v0.9 UI Foundation Design

## Status

Approved direction: Approach A — incrementally extend the existing theme system and shared UI components.

## Goal

Establish the v0.9 `Cyrene Ripple / 昔涟·涟漪` visual foundation without changing page business logic, routing, Agent execution behavior, or SSE event semantics.

## Context

The frontend already has a ThemeProvider, persisted theme configuration, preset migration, CSS custom properties, Tailwind aliases, and shared UI components. v0.9 must build on those boundaries instead of introducing a second theme engine or rewriting individual business pages.

The existing public navigation and runtime contracts must remain compatible. In particular, UI work must not change the meaning or handling of `token`, `tool_start`, `tool_end`, `approval_required`, `done`, or `error` SSE events.

## Scope

Phase 1 includes:

1. Add the Cyrene Ripple preset and make it the default preset for new installations.
2. Preserve legacy preset IDs and stored theme data through normalization/migration.
3. Add semantic CSS variables for the v0.9 palette, surfaces, typography, borders, states, radii, shadows, and motion timings while retaining legacy variable aliases used by existing pages.
4. Update the global stylesheet so the default shell, surfaces, focus states, scrollbars, Markdown content, and reduced-motion behavior use the new semantic tokens.
5. Align the existing Button, Input, Textarea, Panel, Badge, Modal, Drawer, PageHeader, EmptyState, and Spinner components with the shared tokens and accessible states.
6. Visually group the existing NavRail entries into `核心`, `能力`, and `系统`; preserve every current label, route, health status indicator, and navigation behavior.
7. Add unit tests for preset defaults, migration compatibility, semantic token application, and the navigation grouping model where the existing test setup supports it.

## Non-goals

- No central homepage redesign or composer relocation in Phase 1.
- No Agent loop, tool registry, approval semantics, API, backend, or SSE changes.
- No voice/TTS, Live2D, browser automation, or new asset-generation pipeline.
- No removal of existing theme import/export, background image, custom variable, or accessibility behavior.
- No broad refactor of business pages.

## Visual Direction

The product balance is 70% desktop productivity application and 30% character theme. The default surface is light and calm; the navigation is deep purple rather than black; character-like decoration remains subtle and must not reduce readability.

Core values:

```css
--bg-app: #f9f7ff;
--bg-soft: #f4f0fc;
--surface: rgba(255, 255, 255, 0.82);
--surface-solid: #ffffff;
--surface-hover: #faf5ff;
--text-primary: #292536;
--text-secondary: #6d6678;
--text-muted: #9b95a6;
--accent-primary: #ea91b9;
--accent-primary-hover: #df7eaa;
--accent-soft: #f9dce9;
--accent-purple: #ad9be8;
--accent-blue: #99cfea;
--accent-gold: #ebcf8c;
--sidebar-bg: #29263a;
--sidebar-bg-2: #312c46;
--sidebar-text: #eeeaf8;
--sidebar-muted: #aaa3ba;
--sidebar-active: rgba(234, 145, 185, 0.17);
--border-soft: rgba(86, 72, 117, 0.10);
--success: #73b99a;
--warning: #d9b866;
--danger: #df7995;
--info: #84bcdc;
```

The existing variables such as `--bg`, `--panel`, `--text`, `--accent`, and `--nav` remain supported. The semantic variables are the preferred names for new and migrated shared components; compatibility aliases map the existing names to the same values.

Radii are constrained to 6, 10, 12, 16, and 20px. Shared shadows are limited to a light card shadow and a stronger floating-layer shadow. Motion uses opacity and transform first, honors `prefers-reduced-motion`, and avoids continuous large blur or backdrop-filter animation.

## Theme Architecture

The existing `ThemeConfig` remains the persistence boundary. `PresetId` gains `cyrene-ripple`; `PRESETS` gains a complete preset; `DEFAULT_THEME` points to it. `normalizeStoredTheme` continues to accept current preset IDs and legacy aliases. Existing saved themes must not be silently discarded.

`applyThemeToDom` remains the single DOM application point. It sets both the legacy variables consumed by current pages and the semantic v0.9 variables consumed by the shared foundation. The custom variable whitelist remains fail-closed; no arbitrary CSS or executable URL values are introduced.

The preset metadata exposes the new theme name and description to the existing Settings appearance section. A separate theme selector or new persistence format is not introduced in Phase 1.

## Shared Components

Shared components keep their current public props unless an accessibility or token requirement needs a backward-compatible optional prop. Their visual contract is:

- Button: primary, secondary, ghost, and danger variants; visible hover, focus-visible, disabled, and reduced-motion behavior.
- Input/Textarea: label, hint, placeholder, focus, disabled, and error-compatible visual states.
- Panel/PageHeader: light surface, subtle border, modest shadow, and consistent padding/radii.
- Badge: default, success, warning, danger, info, and accent tones using semantic state tokens.
- Modal/Drawer: existing keyboard close and focus-return behavior retained; overlay and panel use v0.9 surfaces.
- EmptyState/Spinner: readable muted states and the new accent without adding character art.

## Navigation Grouping

NavRail retains all current routes:

```text
核心
  对话
  任务（若该入口存在于当前 runtime）
  工作流
  工作空间

能力
  技能
  插件
  能力
  知识库
  记忆中心（若该入口存在于当前 runtime）

系统
  监控
  日志
  设置
```

The implementation must derive visible entries from the existing route definitions rather than inventing routes. If an item listed in the visual proposal is not currently registered, it is not added in Phase 1. Group labels, subtle dividers, active pink-purple background, left highlight, and small sparkle treatment are visual-only.

Backend health remains visible and continues to poll at the existing interval.

## Testing and Verification

The implementation follows red-green-refactor for each new behavior. Tests cover:

- `cyrene-ripple` is the default preset.
- Existing legacy preset migrations still resolve to their current targets.
- Semantic variables are applied together with legacy compatibility variables.
- Existing custom variable sanitization remains restrictive.
- Navigation grouping preserves all currently registered routes and labels.

Required verification before delivery:

```powershell
cd frontend
npm.cmd test
npm.cmd run build
```

The final review must also include a browser-equivalent visual check of at least the chat shell and one non-chat page, with explicit limitations if direct Tauri WebView evidence is unavailable.

## Acceptance Criteria

Phase 1 is accepted when:

1. A fresh frontend load visibly uses the Cyrene Ripple theme.
2. Existing persisted themes and legacy IDs still load without data loss.
3. Shared components look consistent across an existing page without page-specific redesign.
4. Sidebar sections are visually separated while all current navigation and backend status behavior remain intact.
5. Reduced-motion and keyboard focus behavior remain available.
6. Frontend tests and production build pass with fresh command output.
7. No backend, Agent, API, route, or SSE behavior has been changed.
