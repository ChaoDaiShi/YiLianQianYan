# YiLian browser adapter

This optional provider exposes one Playwright-owned browser context through MCP stdio. It never attaches to a user's existing browser. It requires Node.js and an installed Chromium-compatible executable; it does not download or install a browser.

Install the locked dependency locally, then configure the MCP server command:

```powershell
cd plugins/browser-adapter
npm.cmd ci
node src/server.mjs --browser-executable "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --allow-origin "https://example.org" --evidence-dir "F:\YiLianQianYan\runtime\browser-evidence"
```

Repeat `--allow-origin` for each permitted origin. An origin is an exact HTTP(S) origin such as `https://example.org` or `http://127.0.0.1:4312`; wildcard hosts and path prefixes are not accepted. `BROWSER_EXECUTABLE_PATH`, comma-separated `BROWSER_ALLOWED_ORIGINS`, and `BROWSER_EVIDENCE_DIR` can provide the same values when the MCP provider manager launches the process. Use the executable path already installed on that machine.

The provider negotiates MCP `2026-07-28` or `2025-11-25`, advertises the seven `browser_*` tools with input and output schemas, and returns structured results. The tools open, read, fill, click, submit, capture, and close. Every observed element gets a session-scoped handle tied to that observation; any DOM change makes the handle stale. Target resolution uses an exact accessible role and name and requires one match. There is no arbitrary JavaScript tool or coordinate fallback.

Each invocation is checked against the configured origins. Playwright request routing also blocks page requests, redirects, and new-page navigation to origins outside that list. The adapter returns page text as observation data and never turns that text into a tool request. The surrounding Agent and Gateway remain responsible for handling prompt-injection attempts in page content.

The provider writes a JSONL evidence stream per session and returns its path with each result. Evidence records include operation and observation ids, time, page URL without query or fragment, and outcome. Filled values are never written to evidence; only their character count is recorded. Screenshots are real PNG files with SHA-256 digests. Keep the configured evidence directory under the application's managed artifact storage so the parent runtime can import the files into its artifact/evidence service.

`browser_submit` only reports that the observed form's submit control was clicked and the resulting page was observed. It does not claim the remote business operation succeeded. If the click result becomes uncertain, it returns `result_state: "unknown"` with an `operation_id` and does not click again. The parent MCP registration must keep the adapter tool calls behind the SecurityExecutionGateway; in particular, classify `browser_submit` as high risk and require the existing approval path. The provider itself does not implement or bypass approval.

## Fixture verification

The tests start a local HTTP fixture that contains an article, a labelled form, a page-mutation control, and adversarial page text. They drive the actual MCP stdio process and installed browser, then verify the page text, input value, one server-recorded submission, and stale-observation rejection.

```powershell
$env:BROWSER_EXECUTABLE_PATH = "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
npm.cmd test
```

The fixture is not an external provider acceptance test. A production browser provider still needs invocation-specific origins and parent Gateway registration.
