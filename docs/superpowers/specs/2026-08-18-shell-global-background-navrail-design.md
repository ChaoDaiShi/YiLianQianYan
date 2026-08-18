# Shell Global Background and Adaptive NavRail Design

## Scope

This iteration updates only presentation and layout for the desktop shell and home composition. Backend, API, store semantics, agent runtime, persistence, security, and existing navigation behavior remain unchanged.

## Visual direction

- Promote the existing Cyrene flower-and-water artwork from the home-only ambient layer to the shell background layer.
- Use translucent cool-lilac overlays so the artwork remains visible without reducing text and control contrast.
- Keep major interactive surfaces lighter and quieter than the background.
- Reuse cropped artwork from the supplied visual sheet rather than introducing generated character art.

## Adaptive NavRail

The NavRail keeps every existing item and route but adapts its presentation to available height:

1. Comfortable height: icon-above-label navigation items with normal spacing.
2. Compressed height: preserve icons and labels while reducing group gaps, item padding, and label line-height.
3. Constrained height: switch to icon-first compact items and expose the full label through the existing accessible name/tooltip path.

The rail is divided into fixed top branding, flexible navigation, and fixed backend-status regions. The navigation region may shrink internally, but the rail does not display a visible scrollbar and does not hide or delete any real navigation item.

## Responsive behavior

- Verify at 1366x768 and 1200x800.
- Verify a lower-height non-fullscreen window to confirm the compact mode activates before content is clipped.
- Keep Workbench as the largest visual region and preserve the existing collapsed execution rail behavior.

## Validation

- Add or update frontend layout-contract tests for global background ownership and adaptive NavRail states.
- Run the frontend test suite, production build, and `git diff --check`.
- Capture actual Tauri screenshots at the fixed sizes and visually inspect the first focus, sidebar completeness, contrast, and scrollbar absence.
