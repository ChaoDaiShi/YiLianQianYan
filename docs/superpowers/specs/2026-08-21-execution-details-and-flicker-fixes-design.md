# Execution Details and Flicker Fixes Design

## Goal

Make execution history details open toward the workbench, keep the application shell visually stable while lazy pages load, and prevent internal Windows child processes from flashing a terminal window in the packaged desktop application.

## Confirmed root causes

1. `ExecutionHistory` renders details inline inside the narrow right rail. The details increase card height and cannot use the wider workbench area.
2. `App.tsx` places `Suspense` above `Routes`. When a lazy route suspends, the fallback replaces `AppShell`, remounting the navigation, ambient background, and animated content viewport.
3. `/api/system` refreshes every three seconds and `collect_system_info()` starts `powershell.exe` on every call to query static GPU metadata. A GUI-subsystem Windows parent does not automatically suppress the console window of a console-subsystem child.

## Design

### Execution history details

- Keep the compact card in the execution rail.
- Render the selected detail panel through a React portal attached to `document.body`.
- Anchor the panel to the left edge of the selected card with a small gap.
- Clamp the panel to the viewport and switch to a viewport-width panel when the left-side space is narrow.
- Permit only one expanded history record at a time.
- Close on the trigger, Escape, or a pointer press outside the trigger and panel.
- Recalculate placement on resize and scroll.
- Preserve the existing real-field-only detail content and accessibility attributes.

### Route loading stability

- Keep route definitions and lazy imports unchanged.
- Move the `Suspense` boundary into the persistent `AppShell` content region so navigation and ambient layers never unmount during route chunk loading.
- Remove the page-wide entrance animation from the persistent viewport.
- Make the route loading surface transparent and delay its subtle placeholder reveal. Fast route loads therefore show no transient card.

### Windows background process behavior

- Add one backend utility that applies `CREATE_NO_WINDOW` to both standard and Tokio commands on Windows and is a no-op elsewhere.
- Apply it to background/internal commands that pipe or discard stdio: system GPU discovery, process listing, MCP stdio/probes, and portable process-control helper commands.
- Keep isolated Agent command execution on its existing Windows isolation implementation, which already uses `CREATE_NO_WINDOW`.
- Cache GPU metadata after the first query because it does not need three-second polling. CPU, memory, disk, health, and the API response shape remain unchanged.

## Testing

- Pure placement tests prove the detail panel is placed to the left and remains inside a narrow viewport.
- Source contracts prove the shell owns the `Suspense` boundary and the route fallback uses delayed reveal styling.
- Backend unit tests prove the Windows background-process flag and one-time GPU cache behavior.
- Run focused tests first, then all frontend tests, frontend production build, `cargo fmt --check`, `cargo check`, and relevant backend tests.

## Scope boundaries

- No SSE, Agent, Store, API schema, database, approval, or security-policy changes.
- No eager loading of every feature page and no new dependency.
- No broad visual redesign beyond the affected detail panel and loading surface.
