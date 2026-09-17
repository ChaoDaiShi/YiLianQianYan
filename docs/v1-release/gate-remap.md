# Gate remap after freezing v2

## Decision

The user froze v2 DesktopSpace development and moved v1 to an independent release lineage. Historical Gate 3 records remain unchanged. A historical `PASS` or `BLOCKED` is not rewritten, and historical evidence is not represented as current v1 branch evidence.

## Old Gate 3 mapping

| Old item | v1 disposition | v1 evidence rule |
| --- | --- | --- |
| G3.1 global voice continuity | Conversation, Task Canvas and other v1 page continuity remains in scope. Desktop switching is `DEFERRED_V2`. | Automated session/generation/anchor tests plus V1-H2. |
| G3.2 task pause/resume | In scope. | Task API, scheduler and live projection tests plus V1-H4. |
| G3.3 task retry/rerun | In scope. | Persisted control, checkpoint, invalidation and attempt-history tests. |
| G3.4 desktop open | `DEFERRED_V2`. | Not a v1 release blocker and not claimed as passed. |
| G3.5 desktop focus | `DEFERRED_V2`. | Not a v1 release blocker and not claimed as passed. |
| G3.6 task status/current query | In scope. | Deterministic router, Task API and narration tests plus V1-H4. |
| G3.7 direct-speech TTS interruption | In scope. | Audio lifecycle automation plus V1-H3; button interruption is insufficient. |
| G3.8 ambiguous voice approval | In scope with fail-closed semantics. | ApprovalStore uniqueness, conversation/identity, generation, lease and idempotency tests; one representative human voice command in V1-H. |
| G3.9 conversation anchor | In scope. | Navigation, deletion/switch, stale generation and persistence tests plus V1-H1/H2. |
| G3.10 provider and playback lifecycle | In scope for v1 voice. | MiniMax STT/TTS tests plus V1-H1/H3/H5. |
| G3.11 Voice-to-Desktop native continuation | `DEFERRED_V2`. | Not a v1 release blocker and not claimed as passed. |

## Replacement release gates

### V1-RC technical gate

- All R1-R9 automatable requirements have evidence on one exact candidate SHA.
- Rust formatting/check/tests, Tauri regression, frontend tests/build and real-backend browser E2E pass.
- Fresh database, supported v0.9 upgrade, repeated startup, corruption refusal and transaction rollback are verified.
- Default startup is the v1 Workspace and does not initialize Windows observation/control loops.
- No secret, user data, local bundle, database, audio, model, internal report or private absolute path exists in new commits or build artifacts.
- The candidate remains a release candidate until human checks finish.

### V1-H human gate

1. Real microphone to MiniMax STT, correct Conversation persistence and audible cloud TTS reply.
2. One VoiceSession across Conversation A, Canvas, Capability/Monitoring and return; explicit switch to Conversation B does not receive A's stale turn.
3. Explicitly enabled hands-free mode interrupts old TTS by direct speech, resists echo self-trigger and releases the device when disabled.
4. Voice status/pause/resume on an independently executing, non-destructive Workflow graph.
5. Representative file-to-task-to-artifact export, voice preview/selection, Canvas and monitoring layout in real Tauri.
6. Candidate installer/startup/permission/normal shutdown experience and honest resource observation.

Until all applicable V1-H items pass, the release status is `HUMAN_PENDING`, never a formal v1.0 acceptance.
