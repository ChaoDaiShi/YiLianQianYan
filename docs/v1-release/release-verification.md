# v1.0-rc.1 release verification

Current status: `HUMAN_PENDING`

The candidate is not a formal v1.0 release. This report records current evidence without promoting historical results to current-SHA proof.

## Candidate identity

- Product version: `1.0.0-rc.1`.
- Branch: `v1/release-work` in the independent v1 release clone.
- Application source and full-gate checkpoint: `8394e4495f500f69c57388ebf8510fafcaecbabb`.
- The later packaging/report-only commit does not change compiled application source. Its exact remote SHA is recorded after the authorized branch push.

## Technical RC evidence

| Area | Command/evidence | Result |
| --- | --- | --- |
| Frontend tests | `npm test` | PASS, 83 files / 389 tests |
| Frontend production build | `npm run build` | PASS; 2,085 modules; non-blocking 500-kB main-chunk warning retained |
| Rust formatting/check | `cargo fmt --all -- --check`; `cargo check --workspace --locked -j1` | PASS; backend and Tauri `1.0.0-rc.1` checked |
| Rust/Tauri full regression | `cargo test --workspace --all-targets --locked -j1` | PASS; Tauri 5/5, backend library 880/880 and every integration target reported 0 failures |
| Migration/recovery coverage | Full Rust gate | PASS; fresh/adopted v0.9/repeat startup, digest/collision/corrupt JSON rejection and transaction rollback tests executed |
| Real-backend browser E2E | `npm run test:e2e` with system Edge and isolated DB/workspace | PASS; first-run setup, two distinct graphs, node, auto-layout, Capability and System routes |
| Version consistency | `scripts/verify-release-version.ps1 -ExpectedVersion 1.0.0-rc.1` | PASS across npm, Tauri, Cargo manifests and lockfile |
| Commit/content boundary | Foundation ancestry plus all-new-commit scans | PASS; no tracked secrets/data/audio/model/database/bundle, private absolute paths or v2 runtime implementation matches |
| Windows RC build | `npm run build:windows -- -SkipTests -Version 1.0.0-rc.1` after the complete gate | PASS; NSIS built and checksum file verified |
| Packaged GUI automation | `scripts/test-windows-gui.ps1` against the RC | PASS; visible 1214×838 window, healthy backend/database, normal close, released port and isolated uninstall |

Installer artifact (unsigned RC):

- Filename: `忆涟千言_1.0.0-rc.1_x64-setup.exe`
- Size: `8,740,354` bytes
- SHA-256: `6255EFB9D6D7EE572A295974827F3F8133ED71A5C52C912ABC03CBF0BAB7E930`
- Local build output: `target/release/bundle/nsis/` (ignored by Git and not uploaded as a Release)

## Dependency review

- `npm audit --omit=dev`: 0 critical, 0 high, 2 moderate. React Router DOM is patched within the 6.x line to 6.30.6. The remaining two records require a semver-major 7.x migration. The SSR hydration advisory is not reachable in this Vite SPA; navigation uses internal literal paths or encoded persisted IDs rather than accepting an arbitrary external destination. Keep this documented risk and re-evaluate before a later router migration.
- Development-only audit findings are not shipped in the frontend bundle; they remain visible in the raw full audit and are not silently described as fixed.
- `cargo audit --json`: 3 advisory records. Both `quick-xml 0.30` denial-of-service records are absent from the Windows dependency graph and occur only as the non-Windows `xcb` build dependency through `xcap`; Windows product/document parsing uses fixed `quick-xml 0.41.0`. `rustls 0.23.42` has no reverse dependency for workspace/all targets and is an unreachable stale lock entry. No reachable high-severity runtime advisory was found for this Windows RC.
- Informational maintenance warnings remain in transitive build/parser trees, including `ttf-parser 0.25.1` via `pdf-extract`; replacement is deferred and not described as fixed.

## Remaining actions

1. Push only `HEAD:refs/heads/v1/release-work`, verify the remote SHA, and do not create a tag or GitHub Release.
2. Complete `v1-h-checklist.md`. Real microphone, hearing, direct-speech interruption, representative file/artifact use and hands-on installer experience remain human evidence.
3. Keep the release at RC/HUMAN_PENDING until all applicable V1-H rows are accepted.
