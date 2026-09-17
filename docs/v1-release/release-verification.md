# v1.0-rc.1 release verification

Current status: `IMPLEMENTING`

The candidate is not a formal v1.0 release. This report records current evidence without promoting historical results to current-SHA proof.

## Candidate identity

- Product version: `1.0.0-rc.1`.
- Branch: `v1/release-work` in the independent v1 release clone.
- Latest functional checkpoint before this report: `0209a50` (`feat(task-world): add persisted groups and review suggestions`).
- Exact final RC commit and installer SHA-256 are recorded after the remaining documentation/security scan and the one full technical gate.

## Current focused evidence

| Area | Command/evidence | Result |
| --- | --- | --- |
| Backend compile | `cargo check -p yilian-backend -j1` | PASS; two pre-existing unused migration-helper warnings |
| Frontend Task World | four focused Vitest files | PASS, 23 tests |
| Frontend production build | `npm run build` | PASS; 2,085 modules; main chunk warning remains |
| Canvas migration | focused `task_canvas_persistence` migration test | PASS; migration 1013 registered and `groups_json` present |
| AI review boundary | focused strict review parser test | PASS; unknown nodes and executor injection reject |
| Retry preservation | focused graph-detail projection test | PASS; existing retry policy survives accepted edits |
| Real-backend browser E2E | `npm run test:e2e` with system Edge, isolated DB/workspace | PASS; first-run setup, two distinct graphs, node, auto-layout, Capability and System routes |
| Version consistency | `scripts/verify-release-version.ps1 -ExpectedVersion 1.0.0-rc.1` | PASS across npm, Tauri, Cargo manifests and lockfile |

These are focused development checks. The final RC gate still requires the complete Rust/frontend/Tauri/migration suite on one exact commit.

## Dependency review

- `npm audit --omit=dev`: 0 critical, 0 high, 2 moderate. React Router DOM is patched within the 6.x line to 6.30.6. The remaining two records require a semver-major 7.x migration. The SSR hydration advisory is not reachable in this Vite SPA; navigation uses internal literal paths or encoded persisted IDs rather than accepting an arbitrary external destination. Keep this documented risk and re-evaluate before a later router migration.
- Development-only audit findings are not shipped in the frontend bundle; they remain visible in the raw full audit and are not silently described as fixed.
- Cargo audit/reachability is rerun on the frozen RC. Earlier focused review found no newly reachable parser vulnerability; `quick-xml 0.30` was non-Windows transitive build-only through `xcap/xcb`, and current product parsing uses pinned `quick-xml 0.41.0`. `rustls 0.23.42` was a stale unreachable lock entry. `pdf-extract` retains an informational unmaintained-font-parser risk for later replacement review.

## Remaining technical gate

1. Scan all commits added after the public Foundation for secrets, user data, v2 implementation and private absolute paths.
2. Run one current-commit complete frontend test/build and Rust/Tauri format/check/test gate.
3. Re-run fresh/repeat/v0.9 upgrade, corruption refusal and transaction rollback through the complete suite.
4. Run current lockfile dependency audits and record reachable/unreachable disposition.
5. Build the Windows NSIS RC, hash it, execute isolated packaged GUI launch/health/normal-close/uninstall acceptance, and record the exact artifact.
6. Push only `HEAD:refs/heads/v1/release-work`, verify the remote SHA, and do not create a tag or GitHub Release.
7. Hand off `v1-h-checklist.md`; keep final status `HUMAN_PENDING` until the user completes physical/perceptual checks.
