# v1.0.0-rc.2 release verification

Current status: `TECHNICAL_GATE_PASS` + `HUMAN_PENDING` + `BLOCKED_BY_WORKFLOW_SCOPE`

This is an unsigned release candidate, not the formal v1.0.0 release. Historical rc.1 binaries, checksums and evidence remain unchanged.

## Candidate identity

- Start HEAD: `38e135437a9d211282420d7c6b981f70d7d6f3ff`.
- Functional application checkpoint: `81afbed6f2cdf09b0590064a4293ae47472ad1bd`.
- Technical/packaging checkpoint: `ac5745553cd6304432785f1ad796e1c04e79582b`.
- Product version: `1.0.0-rc.2`.
- Source branch during verification: `fix/v1-rc2-provider-voice-ux`.
- Intended integration branch: `v1/release-work`.

## Fixed blockers

| Area | Result | Current evidence |
| --- | --- | --- |
| Settings overlap | PASS | Real-backend Edge E2E at 1280×720, 1366×768 and 1920×1080; Save remained visible/reachable, no horizontal clipping, Voice Pill did not overlap Save |
| Model Secret persistence | PASS | Write-only redacted GET, omission keeps the ref, explicit clear deletes, restart persistence/provider resolution test passed, delete failure preserves the model row |
| Provider connection test | PASS | Protected real leaf route uses existing model/STT/TTS adapters and only six normalized, value-safe error codes |
| Model unavailable guidance | PASS | Chat, Task planning and AI graph review stop before invocation and show a model Settings link |

## Voice UX

| Area | Result | Current evidence |
| --- | --- | --- |
| Chat voice input | PASS (automated) | Existing real capture runtime is wired into both composer states; final-first STT inserts editable text and does not auto-send |
| Global Voice | PASS (automated) | Product-safe Pill/panel, existing single GlobalVoiceSession and automatic Global Voice TTS |
| Cross-route session | PASS (automated) | The host remains mounted above v1 routes and updates context without replacing the session |
| Direct-speech barge-in | PASS (automated), HUMAN_PENDING (physical) | Global entry enables existing hands-free monitor; local speech onset interrupts playback before the next lease/STT |
| Echo protection | PASS (automated), HUMAN_PENDING (acoustic) | Playback/session/generation/lease-bound echo evidence and final dispatch checks passed |
| Manual Chat TTS | PASS (automated), HUMAN_PENDING (audible) | Assistant playback is explicit and does not write Conversation history; ordinary Chat does not auto-play |
| Media release | PASS (automated), HUMAN_PENDING (device indicator) | Cancel/error/unmount/session-end tests release tracks, abort requests, cancel animation frames and close AudioContext |

## Security

- Settings/model APIs do not return plaintext Secret values.
- Provider response bodies, Authorization values and Secret values are not emitted by the new UI/log paths.
- Commit boundary scan found no real credential-shaped additions, private absolute paths, databases, audio/model blobs, installers or v2 runtime paths.
- Voice continues through the existing Interaction Router, generation checks, approval attestation and Security Gateway; no approval or execution bypass was introduced.

## v1.1 roadmap

- Optional Identity: `DOCUMENTED_ONLY`.
- Cloud Model Gateway: `DOCUMENTED_ONLY`.
- Cloud Harness: `DOCUMENTED_ONLY`.
- Web Surface: `DOCUMENTED_ONLY`.
- v1.1 implementation started: `NO`.

## Technical gate

| Area | Command/evidence | Result |
| --- | --- | --- |
| Frontend tests | `npm test` | PASS, 90 files / 422 tests |
| Frontend production build | `npm run build` | PASS, 2,089 modules; retained non-blocking 500-kB main-chunk warning |
| Rust formatting/check | `cargo fmt --all -- --check`; `cargo check --workspace --locked -j1`; `cargo check -p yi-lian-qian-yan --locked -j1` | PASS; backend and Tauri `1.0.0-rc.2` checked |
| Rust/Tauri regression | `cargo test --workspace --all-targets --locked -j1` | PASS; Tauri 5/5, backend library 892/892 and all integration targets reported 0 failures |
| Migration/recovery | Full Rust gate | PASS; fresh/adopted/repeat startup, ownership/digest/collision/corrupt-state/rollback coverage executed |
| Real-backend browser E2E | system Edge, isolated DB/workspace/backend port | PASS; two graphs, protected routes, Global Voice entry and all three Settings viewports |
| Version consistency | `scripts/verify-release-version.ps1 -ExpectedVersion 1.0.0-rc.2` | PASS across npm, Tauri, Cargo manifests and lockfile |
| Commit/content boundary | `38e1354..ac57455` changed-path/content scan | PASS; 55 tracked paths, no forbidden artifact/private path/credential-shaped/v2 runtime match |
| Dependency review | `npm audit --omit=dev --json`; `cargo audit --json`; explicit Windows reverse trees | REVIEWED; no high/critical npm issue and reported Rust vulnerable versions are absent from the Windows dependency graph |

The first E2E launch found port 9420 already occupied by an independently running YiLian process. The test harness was updated to accept an isolated backend port without widening CORS; the successful rerun kept the allowed frontend origin at port 1420.

## Dependency risk retained

- npm production audit: 0 critical, 0 high, 2 moderate React Router records. The available fix is a semver-major v7 migration. Current navigation targets are internal literals, UUIDs or encoded persisted IDs; SSR hydration is not used by this Vite SPA.
- cargo audit: three records in the cross-platform lockfile. Explicit `x86_64-pc-windows-msvc` reverse-tree checks print no path for `rustls 0.23.42` or `quick-xml 0.30.0`. The Windows parser/runtime graph uses patched `quick-xml 0.41.0`. Informational unmaintained transitive dependencies remain documented rather than silently described as fixed.

## Installer

- Filename: `忆涟千言_1.0.0-rc.2_x64-setup.exe`.
- Size: `8,838,102` bytes.
- SHA-256: `3B4FA8A73C5A155CCC447B81B094D3A268CDFBAEA29360D01B478C2FA37BE406`.
- Signing: unsigned.
- Build: PASS after the complete source gate, using `npm run build:windows -- -SkipTests -Version 1.0.0-rc.2`.
- Packaged GUI automation: `BLOCKED_EXTERNAL` because an independently running YiLian process owns the product-fixed port 9420. It was not terminated by the release task. Run `scripts/test-windows-gui.ps1` after that process exits.

## Release status

- Technical source gate: `PASS`.
- Installer build and checksum: `PASS`.
- Source/installer candidate freeze: `FROZEN`.
- Packaged GUI automation: `BLOCKED_EXTERNAL` as recorded above.
- Remote CI: `BLOCKED_BY_WORKFLOW_SCOPE`; no remote CI success is claimed.
- Human acceptance: `HUMAN_PENDING`; use the current `v1-h-checklist.md`.
- Formal v1.0.0: `NOT RELEASED`.
