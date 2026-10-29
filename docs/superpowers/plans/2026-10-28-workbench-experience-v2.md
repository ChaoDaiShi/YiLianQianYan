# Workbench Experience v2

User authorized implementation across chat, canvas, memory, skills, model/services, research, density, floating windows and system tray. Subagents use `gpt-6.1-terra` / `high`. No defensive fallback layers or intermediate tests/builds; full validation begins only after integration. Existing unrelated `.agents/skills/` and `artifacts/` remain untouched.

Baseline: `7bd8b3e`, branch `codex/workbench-experience-v2`, fetched from `origin/develop`.

## Required outcomes and ownership

- Chat/canvas worker: genuine provider thinking display and history; attachments-only user messages; shared chat composer in canvas; remove duplicate transcript panels; non-overlapping new branches and latest-conversation focus.
- Model/services worker: unified provider/account/model/key form; service setup without pool prerequisites; capability-linked multimodal profiles; DeepSeek Flash vision and standalone Search/Fetch; model pools as `model-pools` advanced child node.
- Native worker: actual Tauri system tray, explicit collapsed floating controls, coherent hide/end behavior, monitor audit follows its explicit URL/action.
- Memory/skills worker: restore actual missing memory overview/filter features from the pre-preview layout while preserving current governance and data; official-first skills from bundled resources; explicit external skill import using existing package preview/install; compact skill toolbar.
- Knowledge worker: deeper multi-angle search and full-page reading with source-backed quotations; preserve quick mode and actual failure/cancellation states.
- Integrator: compact setting-node geometry and camera, advanced-node wiring and shared DTOs; packaged official resource paths; native workflow drag/drop and zoom geometry; scoped density/alignment across related management views; final review and validation.

## Confirmed external contracts

- DeepSeek `deepseek-flash` supports image input using Chat Completions user image content: https://api-docs.deepseek.com/guides/vision/
- Thinking is returned as `reasoning_content`; tool-bearing multi-turn requests retain the supplied reasoning: https://api-docs.deepseek.com/guides/thinking_mode/
- Standalone Search/Fetch use the same bearer account credential at `/v1/tools/search` and `/v1/tools/fetch`: https://api-docs.deepseek.com/guides/search/ . Built-in Responses web search is a separate model capability.
- Tauri Windows HTML drag/drop requires disabling the native WebView drag/drop handler: https://v2.tauri.app/reference/config/

## Integration decisions

- All provider capabilities/credentials and execution outcomes remain real API data; no synthetic conversation, mock canvas scene or default demo records in production.
- External acceptance fixtures and screenshots live outside the repository. No real user profiles or credentials are used for controlled verification.
- System tray exposes show/hide main window, show/hide existing floating windows, and explicit exit. Main-window close moves to tray; ending a call is separately labeled from hiding its window.
- Official skills are bundled as distinct declared resources; source metadata identifies them. User skill folders and external imports remain separately editable according to existing ownership rules.
- Model pool defaults and saved data remain supported as explicit advanced settings; the new ordinary setup path creates provider/model/service records without requiring a pool.
- Memory restoration targets functionality removed in `28522ad`, not a wholesale source/database rollback.

## Final validation (not started during implementation)

- Frontend full tests and production build; Rust formatting, workspace check and relevant/full tests; Tauri check/build.
- Browser interaction on isolated real backend: ordinary and thinking conversation, attachment-only display, shared canvas composer, branch placement/latest focus, model/account/service creation and pool entry, research progression, memory features, official/external skills, monitor/audit navigation.
- Responsive screenshots: 1366x768, 960x768, and narrow layout where applicable; compact controls remain aligned and readable.
- Windows native verification: tray menu/click, close-to-tray/reopen/quit, collapsed floating close/hide, HTML node drag/drop. Record limits honestly where the environment cannot provide evidence.
- Preserve all unrelated uncommitted work; Git delivery is not part of this new implementation request.

## Implemented and observed acceptance

- Frontend full suite: 168 files / 795 tests passed after integration fixes. Production build passed with existing mixed dynamic/static HTTP-import and chunk-size warnings.
- Settings canvas: actual 104x104 central square and 180x64 category cards, centered icon, denser geometry and initial fit; advanced model pool editor is directly reachable and declared as a child of Models & Services. Narrow header actions wrap instead of clipping.
- Skills: official three-skill bundle, independent source identity and server-side ownership for official records; external import reuses existing package preview/install. The embedded toolbar now includes search, source, count and actions together. In light/dark 1366/960 views, control centers have zero measured offset; 390 views wrap controls. An isolated portable external Codex fixture completed preview, install, enable and discovery after restart; supporting binary file bytes matched, and its source remained non-official. The skill itself was not executed.
- Conversation acceptance used isolated application data and a local model fixture: attachment-only user content is persisted as an empty string; actual streamed reasoning and final replies survive reload. User canvas nodes do not render model reasoning. A 4.5-second stream survives at least two polling intervals, replaces the temporary node with the persisted record, and never saves temporary IDs in layout API requests. Re-entering overview and specific canvases selects the newest answer and uses the same answer as the continuation parent.
- Model/account/service acceptance covered one-form creation, an intentional verification failure and manual retry without duplicate profiles, shared-account DeepSeek Flash vision defaults and standalone search. These are protocol/flow checks, not commercial-provider authentication evidence.
- Knowledge research acceptance: 18 searches across 3 rounds, 47 unique fixture sources within the 48-source budget, 12 Fetch requests (4 per round), one synthetic account throughout. Round 1/2/3 quotations remain verbatim in their own source records. A controlled Fetch 503 produces an explicit failed build while retaining 15 discovered sources and zero completed rounds.
- Native Windows acceptance: real tray icon located through the notification area, window close hides while the process/API remain alive, actual tray invocation restores foreground, actual menu show/hide actions change window visibility, and explicit tray exit stops the process/API. Collapsed call and monitor windows are 88x64 with operable close buttons. Hiding the call window emits no end event; the explicit end control emits the existing end command. No live microphone/call session was created.
- Bundled skill files under the debug resource directory match the source hashes. Workflow drag/drop was exercised in the browser and the actual Windows WebView: trusted drag events created a node at 80% zoom, and an 80x48 screen-pixel drag moved its canvas position from (204,169) to (304,229). The test graph was not saved or executed.
- Test-created accounts and credentials were cleared. Test-only data, screenshots and scripts live outside the repo under `F:/项目开发/忆涟千言/experience-v2-qa-20261028/`. Host skill discovery is read-only; application state and imported fixtures are isolated.
- Rust formatting, workspace check and backend/Tauri debug builds passed. Final `cargo test --workspace -- --test-threads=1` passed: 43 targets, 1523 passed, zero failed/ignored. An earlier default-concurrency run returned one mock LLM HTTP 502 in the robot cancellation test; that case passed alone and in the full serial run, while the exact concurrency cause remains unconfirmed. No production fallback was added for it. Existing compile warnings remain.
- Final `git diff --check` passed. The 347 previously untracked user files were compared against their recorded SHA-256 values and remain unchanged. Changes remain uncommitted on `codex/workbench-experience-v2`.
