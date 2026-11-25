# Local reparent lock correction

## Provenance and boundary

- Upstream crate: `tauri-runtime-wry` **2.11.4**, unchanged package version and dependency versions.
- Published crate checksum: `4e6fac707727b7a2f48e4ded90976324267371073edbb415ffb73bb0458d203f`.
- Upstream VCS commit recorded by the published crate: `ca90b46b2e2cbbc981dae1b809f4af4343fe0558`, `crates/tauri-runtime-wry`.
- Copied from the exact locally installed published crate. Both `LICENSE_APACHE-2.0` and `LICENSE_MIT`, original package manifests, build script, sources, README and `.cargo_vcs_info.json` are retained. Cargo cache marker `.cargo-ok` and the packaged standalone Cargo.lock are omitted.
- The root workspace excludes this vendor directory and selects it with `[patch.crates-io]`. The root lockfile changes only this same-version package from registry source to local path; no crate upgrade or added dependency.
- No shared Cargo registry files are edited. No raw Win32 pre-parenting, alternative WebView, retries or timing sleeps are introduced.

## Why this patch is necessary

Actual repeated Windows/WebView2 acceptance hung after trusted chrome Controller.MoveFocus completed and before supported `Webview::reparent` returned. The published dispatcher held its `window_id` mutex across `send_user_message` and `rx.recv`. The UI handler performs native Wry reparent, whose synchronous focus callbacks read the same mutex. Consequently the worker can wait for the UI while the UI waits for that worker's identity lock. An earlier focus method acknowledgement did not eliminate later native event delivery.

## Source changes

`src/webview_parent.rs` is a private shared parent state: one mutex protects the WindowId and a separate dispatch gate serializes reparent operations and ordinary message enqueue. It exposes the existing `lock()` identity access for callbacks. Its reparent helper snapshots the ID, releases the identity guard, invokes the existing native send/receive operation, and updates the ID only after success. A native error leaves the old identity unchanged. Native callbacks never acquire the dispatch gate. Ordinary dispatch uses a private `dispatch_id()` RAII guard that holds the gate and ID only for the same enqueue expression that previously held the ID guard; getters release it before waiting for a reply. This prevents an ordinary request from snapshotting the old parent and being enqueued behind an in-flight move.

`src/lib.rs` routes the existing dispatcher reparent implementation through that helper without changing the native Reparent message, UI handler, error propagation or public API. All 25 ordinary dispatcher identity expressions and the shared webview-getter macro use the enqueue guard. Native callbacks retain their short identity-only lock. Five existing Arc allocations now construct this shared state; three type sites (dispatcher, native creation and synthetic IPC handler) carry it. All five dispatcher literal constructors retain their existing parent-state allocation layout. The WindowChild path used by browser content shares one Arc across native callbacks, the returned dispatcher, and synthetic IPC dispatchers; derive(Clone) shares that Arc and its dispatch gate. Upstream WindowContent creation has separate returned-dispatcher and native-callback ID allocations; this patch preserves that pre-existing layout rather than changing window construction.

Four helper regressions exercise the exact production lock boundary: callbacks can read identity while a move waits and errors preserve identity; cloned dispatchers serialize moves and the second snapshots the first committed parent; ordinary enqueue waits for the committed parent while native callbacks remain unblocked; getter enqueue releases its guard before waiting for a reply. These are lock-level tests, not a substitute for actual WebView2 acceptance.

## Validation

Root-coordinated verification passed against the local path patch:

- `cargo test --offline -j 1 -p tauri-runtime-wry --lib webview_parent::tests`: **4 passed**, zero failures/ignored.
- `cargo fmt -p yilian-backend -p yi-lian-qian-yan -- --check`: exit 0.
- `cargo check --offline --locked -j 1 --workspace --all-targets`: exit 0.
- `cargo build --offline --locked -j 1 -p yi-lian-qian-yan --features native-browser-fixture --bin yilian-native-browser-fixture`: exit 0.

Verified native fixture SHA-256: `9B1D49BFB042F461ECD61F2AFF1A4E5AB55216EBCB984DF298AFEEBE1E1098AF`.

Actual Tauri/WebView2 acceptance on that binary: core concurrency **6/6 PASS** (`yilian-browser-concurrency-yJ2EVD`), late-created-shell cleanup **1/1 PASS** (`yilian-browser-concurrency-a3XF74`), and **three consecutive 25-group normal native acceptance runs PASS** (`yilian-native-browser-LvZ6A9`, `yilian-native-browser-umQRd1`, `yilian-native-browser-bLTEY9`). Every run has `evidence/result.json` and `evidence/cleanup.json` under its isolated Temp directory. The concurrency runs each report three fulfilled owned-child cleanup entries; normal runs each report four fulfilled entries, zero deferred failures and zero unexpected console errors. These runs preserve the same WebView through reparent and exercise real native visibility, close acknowledgement, focus/IPC behavior and owned cleanup.

Central logs are `browser-fixes/verification/sdk-results.json`, `sdk-helper-tests.log`, `sdk-native-results.json`, `sdk-native-core.log`, `sdk-native-shell.log`, and `sdk-native-normal-{1,2,3}.log`. The wider full workspace test run is separate and was still running when this native validation record was finalized.

Keep upstream two-space formatting. Do not run broad workspace formatting across the vendor package; application formatting uses explicit application package selection.

## Review and removal

Review this patch whenever Tauri/runtime/Wry versions change. Remove the root path patch and vendor directory only after the selected upstream runtime fixes the same callback lock cycle, preserves reparent serialization/error behavior, and passes the same actual native concurrency and repeated normal-acceptance fixtures. Retaining the same crate version does not imply unmodified upstream behavior; this file and the root source override are intentional provenance markers.
