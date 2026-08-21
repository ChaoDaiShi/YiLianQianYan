# Cyrene Day/Night Theme and Surface Design

## Goal

Consolidate the v0.9 UI around the existing “昔涟 · 涟漪 / Cyrene Ripple” visual identity. The application exposes one theme family with light and dark appearances, supports following the operating-system color scheme, and lets the shell artwork remain visible through page-level surfaces.

This change fixes the shared visual foundation. It does not redesign every page's information architecture.

## Decisions

### Theme choices

The appearance setting exposes exactly three choices:

- 跟随系统
- 白天
- 夜间

“跟随系统” is a behavior, not a third visual theme. The only rendered palettes are Cyrene light and Cyrene dark.

The existing warm, neutral, graphite, high-contrast, and custom-theme choices are removed from the visible UI. Their identifiers may remain in compatibility code only long enough to normalize existing local settings safely.

Background image upload and removal remain available. Custom CSS variables, theme import/export, panel-opacity controls, blur controls, font-size controls, and alternate preset controls are hidden from the appearance page in this phase.

### Compatibility

The existing ThemeProvider remains the single theme system. No parallel provider or second token registry is introduced.

Stored legacy presets normalize as follows:

- existing Cyrene Ripple settings become `light` unless a new explicit mode exists;
- known dark presets become `dark`;
- other legacy or custom presets normalize to `system`;
- malformed values normalize to `system`.

No backend, API, database, Store, SSE, Agent, approval, or security behavior changes.

## Theme Model

Introduce a persisted mode:

```text
ThemeMode = system | light | dark
```

ThemeProvider derives an effective scheme:

```text
system → window.matchMedia("(prefers-color-scheme: dark)")
light  → light
dark   → dark
```

The provider subscribes to operating-system scheme changes only while mode is `system`, cleans up the listener, and applies the resolved scheme to the document root through a stable data attribute and compatibility `light` / `dark` classes.

The current background-image storage pipeline remains unchanged.

## Token Architecture

Keep semantic tokens as the contract consumed by components. Define complete light and dark Cyrene token maps rather than deriving the dark palette from arbitrary legacy presets.

The maps cover:

- application canvas and ambient overlay;
- primary, secondary, elevated, hover, and solid surfaces;
- titlebar and navigation surfaces;
- primary, secondary, faint, and disabled text;
- borders and dividers;
- pink, purple, blue, and gold accents;
- success, warning, danger, and information states;
- focus rings, shadows, code surfaces, and background-image masks.

Dark mode uses deep violet and blue-black surfaces, never pure black. Pink remains an accent rather than becoming the page background. Code blocks retain a neutral high-contrast surface in both modes.

Legacy variables continue to be emitted for components not yet migrated, but their values come from the resolved Cyrene palette.

## Canvas and Surface Rules

### Canvas ownership

AppShell is the only full-window background owner. It renders:

1. the selected solid or uploaded background image;
2. Cyrene ambient artwork;
3. a light- or dark-specific readability overlay.

Page roots and full-height content regions remain transparent. They must not paint `--bg-app`, `--surface-solid`, or an almost-opaque gradient across the complete viewport.

### Surface hierarchy

Use four practical levels:

1. **Canvas** — transparent page root, showing the shell background.
2. **Section surface** — low-opacity grouping for page headers, rails, and major split panes.
3. **Card surface** — moderately opaque cards and list/detail containers.
4. **Solid surface** — inputs, menus, dialogs, code, and content that needs guaranteed readability.

Borders remain subtle. Full-page backdrop filters are forbidden. Blur is limited to bounded headers, cards, dialogs, and rails; large surfaces rely on translucent colors and gradients instead.

### Known root-background fixes

- Memory Center no longer ends its page background with opaque `--bg-app`.
- Workflow graph keeps its grid but paints the grid over a translucent canvas layer.
- Active conversation removes the near-opaque full-region gradient and uses a quiet translucent wash.
- Task, Workspace, Capability, Knowledge, Monitor, Logs, and Settings roots inherit the transparent page canvas.
- Shared PageHeader and Panel remain bounded surfaces and receive scheme-safe translucency.

## Appearance UI

The appearance page contains:

- a compact segmented or card selector for 跟随系统 / 白天 / 夜间;
- a short label showing the currently resolved appearance when following the system;
- background image upload and clear controls;
- reset-to-default, which restores `system` and removes the uploaded background.

Alternate preset cards and advanced theme JSON controls are not rendered. Existing theme import/export functions can remain internal during compatibility cleanup, but are not user-facing.

All controls remain keyboard accessible, use `aria-pressed` or radio semantics, and retain a visible focus indicator.

## Motion and Performance

Theme changes transition color, border color, background color, and opacity over the existing short motion tokens. Do not animate full-screen blur, continuously animate shadows, or add particles.

`prefers-reduced-motion: reduce` disables non-essential transitions. Background-image readability uses static overlays.

## Validation

Automated tests cover:

- legacy setting normalization;
- mode persistence;
- system scheme resolution and listener cleanup;
- light and dark semantic token completeness;
- old preset names absent from the appearance UI;
- advanced custom-theme controls absent from the appearance UI;
- page-root contract forbidding opaque application backgrounds;
- existing component/theme tests;
- full frontend tests and production build.

Manual visual checks cover light and dark mode at 1920×1080, 1600×900, and 1366×768, with and without an uploaded background image. The shell artwork should remain perceivable, while text, forms, code, approval states, and status colors remain readable.

## Out of Scope

- Backend changes.
- New theme families.
- Per-page information-architecture redesign.
- New artwork or character assets.
- Animated backgrounds, Canvas particles, or WebGL.
- Replacing the existing ThemeProvider.
- Removing legacy compatibility types before all consumers are migrated.

## Follow-up

After this shared foundation is verified, pages that remain visually weak should be handled in small page groups using the same Canvas and Surface contract. The first follow-up should be chosen from real light/dark screenshots rather than another global CSS pass.
