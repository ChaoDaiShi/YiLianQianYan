# Chat Reliability R1 — implementation and ownership contract

Baseline: dabf3fc9552574f743cfd51e7b6568a667ac11a4.
Branch: fix/v1-chat-init-race; worktree: YiLian-v1-chat-reliability.

## Plan

- [x] R0: mount the real ChatView in system Edge with controlled API/leaf components;
  optimistic send → connected → held history → live completion → release stale history.
  Observe the real messages prop losing the assistant before editing production.
- [ ] R1: add a small conversation ownership clock next to ChatView. Epoch changes on
  identity change, mutation revision changes synchronously for local/live changes,
  hydration requests carry their own generation. Guard messages, execution and
  approval/voice hydration. Keep reducer, transport, backend and UI unchanged.
- [ ] R2: focused component cases for old history, switch A/B, refresh, active tools,
  approval/anchor, terminal ordering, error and stop; pure admission cases A–D.
- [ ] R3: remove beforeStream hydration wait; real backend + immediate loopback +
  system Edge, 10 new conversations in one browser, held old snapshots, persistence
  and actual optimistic/live/persisted IDs. Save timeline and failure diagnostics.
- [ ] R4: full frontend tests/build once, core without Canvas replay, U2, R1;
  report/protected-path review/small commits/authorized push. Stop.

## Ownership decision

History is a snapshot, not the owner of live state. No message-id merge is used:
frontend optimistic user IDs, backend persisted user IDs, engine done IDs and
persisted assistant IDs are generated separately (read-only backend inspection).
An uncontested idle snapshot may replace messages, so reopening and explicit idle
refresh use authoritative persistence without doubling messages.

A snapshot may write only when its conversation epoch, request generation and
live revision still match. A request started while a live turn owns the view is
also ineligible. The first load caused by promoting a local draft preserves its
local messages even if connected/token/done were batched before the load started.
Later idle refresh remains allowed. Ineligible history never replaces execution
records or reconciles older approvals over newer live events. Voice anchor updates
require current identity/request, independently from message snapshot eligibility.

No timeout, sleep or animation-frame wait is part of the production fix.
Existing completion-badge timer is unrelated and retained.
No Backend/Cargo/schema/migration/REST/SSE wire/CSS/Canvas/voice runtime changes.
