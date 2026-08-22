# Restore Avatar-Free Chat Design

## Goal

Restore the legacy conversation presentation where assistant messages do not show an AI avatar or reserve avatar space.

## Behavior

- Assistant messages remain left-aligned with the current message bubble, Markdown, copy action, and tool-call cards.
- User messages remain right-aligned.
- Streaming assistant output and the waiting indicator do not render an AI avatar.
- Tool results and tool-call cards align with the assistant message column rather than an avatar offset.
- No message data, SSE behavior, or execution state changes.

## Verification

Add a frontend render-contract test covering the absence of assistant avatar markup and offsets, then run the focused test, full frontend suite, production build, and backend regression checks.
