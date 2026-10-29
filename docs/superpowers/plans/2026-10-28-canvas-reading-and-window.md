# Canvas reading and startup window

User requested a better default conversation-canvas view, edge connection positions, a right-side selected-message panel, softer conversation/input surfaces, and a startup size closer to a large Codex desktop workspace. This is additive to the uncommitted `codex/workbench-experience-v2` work; no branch switch, Git delivery or replacement of the earlier installer is requested.

## Decisions

- Both overview and specific message canvases share edge ports and a selected-message inspector. Nodes show bounded previews; the inspector reads full Markdown, actual assistant reasoning and original attachments.
- Selecting a node opens its inspector; closing it deselects the node without cancelling ongoing execution. Input/stop remains available in its stable left-column region.
- Initial camera fits a recent ancestor path; explicit conversation selection fits its family, while Fit Canvas shows the whole projection. Opening the inspector does not unnecessarily recenter an already-visible node.
- Edge endpoints follow the actual relative node positions. Vertical chains connect bottom/top, horizontally separated branches connect facing sides, including nodes moved left/above their parent. No persisted node/edge schema is changed.
- New automatic rows leave more room between cards. Existing user positions remain in the saved layout.
- Rounded cards, a stable selected outline and an inset two-row composer use the current Cyrene tokens, actual shared chat controls and local scrolling.
- No measurable visible Codex main window was found in this execution environment. The explicit startup target is therefore 1440x900 logical pixels, constrained by the current monitor work area and DPI, centered before being shown. Existing minimum 800x600 is retained; tray restore never resets a manual resize.

## Ownership

- Integrator: conversation/message canvas components, shared geometry/ports, selected-message panel, canvas CSS and camera behavior.
- GPT-6.1 Terra / high worker: native startup geometry and Tauri window configuration only.

## Final validation

Implementation precedes validation, following the user's existing no-intermediate-testing boundary.

- Frontend test suite and production build; focused edge-direction tests.
- Rust formatting, workspace compilation, relevant startup tests and the message-graph projection regression. Browser verification exposed missing history reasoning in the specific graph; it now reuses the existing conversation reasoning map per run, without changing storage or schemas.
- Browser: real existing-message projection through isolated app data, right inspector click/change/close, full content/attachments, source roles, initial visible context, dynamic edge handles, continued streaming/stop and responsive layout.
- Native: measured startup geometry/work area/DPI and tray reopen preserving manual size, using isolated app data and no user credentials.
- Preserve unrelated user files; test scenes, scripts and screenshots stay outside the repo. The existing installer remains the previously built version until packaging is requested again.

## Acceptance

- Frontend: 170 files / 803 tests passed; final production build passed (2334 modules), with existing mixed-import and chunk-size warnings.
- Rust: formatting and workspace check passed; 9 connector-message database tests passed, including assistant history reasoning parity with the overview and no attribution to user records. Five desktop startup tests and the debug desktop build passed.
- Both canvases: real isolated message records, selected user attachments, full assistant Markdown/history thinking, inspector close, and continuation-parent identity verified. The selected inspector scrolls locally. Default recent context and explicit full-graph fit remain separate commands.
- Edge geometry: actual SVG endpoints sit about 3px from the expected card edges. Vertical, right branch, mouse-dragged left and mouse-dragged above paths use opposing handles; 99 sampled points per tested path stayed outside its source and target cards. Saved positions survived reload. An early missing-edge observation did not reproduce and subsequent same-route/fresh loads showed the expected complete edge set without warnings.
- Long composer regression was reproduced and fixed: all workspace modes share the same two-column grid; attachments and text have bounded local scrolling. At 960x768 with the inspector open, work mode, 20 text lines and four actually uploaded test files, input and actions remain visible, canvas height is about 333px, and model/approval popovers are not clipped. A controlled SSE/stop fixture verified the stop control after closing the inspector; it does not establish a real provider cancellation.
- Native: user explicitly authorized normal exit of the existing portable instance occupying port 9420. It exited through its own tray menu; its files and shortcuts were not replaced. The isolated new app on a 1920x1080 work area at DPI120/125% produced a centered outer window 1728x972 at (96,54), exactly 90% of the work area. Actual inner size was 1710x925 physical pixels (1368x740 logical). After a manual outer resize to 1100x750, close-to-tray and real tray restore preserved that size. Test app, backend port and preview/CDP services stopped.
- Other physical DPI settings, multiple-monitor arrangements and work areas smaller than the retained minimum window were not exercised. The app was not re-packaged or committed in this iteration.
- Evidence/scripts: `F:/项目开发/忆涟千言/canvas-reading-qa-20261028/`. No real model account, paid provider or user message database was used; scenes and test attachments remain external to the product.
