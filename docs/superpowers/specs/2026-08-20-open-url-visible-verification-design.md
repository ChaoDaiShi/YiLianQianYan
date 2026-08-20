# Open URL Visible Verification Design

## Problem

Website-opening requests currently use the generic `bash` tool. On Windows the
PowerShell launcher runs inside a kill-on-close Job Object, so a newly spawned
browser process can be terminated when the launcher exits. The generic verifier
then treats any successful shell exit code as verified success, even though no
browser window was observed.

## Goals

- Open HTTP and HTTPS URLs through the user's configured default browser.
- Keep arbitrary shell execution inside the existing managed-process boundary.
- Report verification success only after observing a visible browser window.
- Replan when launch is rejected or no visible browser window can be observed.
- Preserve existing Agent, approval, audit, API, database, and SSE semantics.

## Design

Add a structured `open_url` tool. It validates the URL as HTTP or HTTPS and uses
the operating system's default URL handler outside the short-lived shell Job
Object. This is a narrowly scoped operation, not an escape hatch for arbitrary
child processes.

The security descriptor declares both network navigation and desktop mutation.
The existing gateway remains the only execution path and continues to decide
allow, approval, or denial before the URL handler is invoked.

After execution, the verifier polls the existing `xcap` window inventory for a
bounded period. Verification passes only when a recognized browser window is
visible and not minimized after the launch. A launcher return code alone never
counts as visible verification.

The `bash` tool rejects recognized URL-launch commands and directs the Agent to
use `open_url` instead. This prevents the existing path from repeating the false
success while still allowing ordinary network-related shell commands. Its Job
Object containment is unchanged.

## Data Flow

1. Agent selects `open_url` with a URL.
2. Security gateway validates the structured descriptor and applies policy.
3. The tool validates the URL and invokes the Windows default URL handler.
4. The verifier observes top-level windows for a bounded period.
5. Existing `tool_end` reports invocation success or failure.
6. Existing `verification` reports visible-window verification success or
   failure; failure requests replanning.

## Error Handling

- Missing or malformed URL: tool error.
- Non-HTTP(S) scheme: tool error.
- Recognized browser-launch command sent to `bash`: tool error that directs the
  Agent to `open_url`.
- OS launcher rejection: tool error.
- No visible browser window before timeout: verification failure and replan.
- Window inventory failure: verification failure rather than false success.

## Testing

- URL validation accepts HTTP/HTTPS and rejects other schemes.
- Bash rejects recognized URL-launch commands without executing them.
- Registry exposes `open_url` and direct execution remains gateway-protected.
- Security descriptor declares the expected permissions, resources, side
  effects, and risk.
- Browser-window matching distinguishes visible, minimized, and unrelated
  windows without launching a real browser in unit tests.
- Verifier passes only with matching observation and fails otherwise.
- Existing gateway, SSE, backend, and frontend regression suites remain green.

## Out of Scope

- Browser automation, DOM inspection, or confirming that a specific page fully
  loaded.
- Relaxing Job Object isolation for Bash.
- New SSE event types or database fields.
- Changes to model prompts beyond the tool's own description.
