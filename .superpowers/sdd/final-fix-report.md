## Final Review Fix Evidence

### Scope and files

- Theme mapping and tests: `frontend/src/theme/applyTheme.ts`, `frontend/src/theme/applyTheme.test.ts`, `frontend/src/theme/presets.ts`, and `frontend/src/theme/types.ts`.
- Generated-CSS-safe shared UI: `frontend/src/index.css` plus Button, Input, Textarea, Badge, Panel, PageHeader, Modal, and Drawer under `frontend/src/components/ui/`.
- Minimum-window navigation: `frontend/src/components/layout/NavRail.tsx`.
- Documentation cleanup: `docs/superpowers/specs/2026-08-17-v09-ui-foundation-design.md`.
- No backend, Agent loop, API, route, SSE, feature, or page business-logic file was changed.

### TDD RED evidence

Command:

```powershell
npm.cmd test -- src/theme/applyTheme.test.ts src/theme/presets.test.ts
```

Result before production edits: exit 1; 2 test files ran, with 4 failed and 9 passed tests. The failures were the expected missing `--bg-soft` value for Cyrene and non-Cyrene themes, custom `--bg` not propagating to `--bg-app`, and a one-key Cyrene/non-Cyrene map mismatch caused by missing `--text-primary` in Cyrene.

### TDD GREEN evidence

The same focused command after the implementation exited 0: 2 test files passed, 13 tests passed, 0 failed. Coverage now includes exact Cyrene values, non-Cyrene mapping, allowed legacy custom override propagation, semantic protection, danger foreground derivation, and complete variable-key parity.

### Final tests and build

- `npm.cmd test -- src/theme/applyTheme.test.ts src/theme/presets.test.ts`: PASS, 2 files and 13 tests.
- `npm.cmd test`: PASS, 15 files and 92 tests, 0 failed.
- `npm.cmd run build`: PASS; TypeScript and Vite completed, 1,846 modules transformed, output CSS `dist/assets/index-DBHvflyk.css` (29.75 kB). Vite reported the existing non-blocking warning that the main JavaScript chunk is larger than 500 kB.
- `git diff --check`: PASS with no whitespace errors; Git printed only LF-to-CRLF working-copy conversion warnings.

### Generated CSS evidence

The generated `dist/assets/index-DBHvflyk.css` contains both explicit declarations:

```css
.shadow-card-token{box-shadow:var(--shadow-card)}
.shadow-float-token{box-shadow:var(--shadow-float)}
```

It also contains the semantic fallback declarations for `--bg-soft: #f4f0fc`, `--accent-primary-hover: #df7eaa`, `--accent-soft: #f9dce9`, `--danger-fg: #292536`, and `--sidebar-active: rgba(234, 145, 185, .17)`. A deterministic source scan across the affected shared UI files and NavRail found 0 occurrences of CSS-variable opacity suffix forms or `shadow-[var(--shadow...)]` reliance.

### Preview smoke

A standalone `vite preview` server ran at `127.0.0.1:4173`. HTTP checks returned 200 for `/`, `/chat`, `/settings`, and `/logs`, each serving the 806-byte application shell. The preview process was stopped afterward and the final listener count on port 4173 was 0.

### Accessibility, safety, and limitations

- Modal and Drawer dialog roles, labels, Escape handling, initial panel focus, focus restoration behavior, backdrop close handling, and component props were retained; their behavior code was not changed.
- Focus containment was not added because this repository's current Vitest run has no DOM environment and the task forbids adding a DOM test dependency. Direct DOM interaction, keyboard-cycle, browser-console, visual screenshot, and Tauri WebView tests therefore remain unavailable in this fix wave.
- The route smoke is HTTP-level evidence, not direct Tauri or visual evidence.
- The design document's extra EOF blank line was removed.
