# V1-H concentrated human acceptance checklist

Status: `HUMAN_PENDING`

Run this checklist only after `docs/v1-release/release-verification.md` records one exact RC commit, a matching installer SHA-256 and a passing technical gate. Historical Gate 3 screenshots or results do not replace these current-candidate checks.

Use one isolated v1 profile. Do not open or migrate the old mixed/v2 database. Do not paste a secret into screenshots or reports; provider settings should show only configured/redacted state.

## H1 — real microphone, cloud STT, persisted reply and audible TTS

1. Start the RC installer build and open a new Conversation A.
2. Start voice input and speak a short unique sentence.
3. Confirm the final transcript matches the spoken meaning and is persisted in Conversation A.
4. Wait for the assistant reply and confirm cloud TTS is audible.
5. Confirm no raw microphone audio appears in history or local export.

Accept only if real microphone capture, MiniMax STT, assistant persistence and audible playback all occur. Button-only input, typed text, mock audio or historical evidence is insufficient.

## H2 — one VoiceSession across v1 surfaces and safe anchor switching

1. With Conversation A anchored, navigate through its Task Canvas, Capability Center, System Monitoring and back to Conversation.
2. Confirm the voice session remains active and the anchor remains Conversation A during ordinary navigation.
3. Explicitly switch to a new Conversation B and speak a second unique sentence.
4. Confirm the second turn persists only in B and no stale A callback appears in B.

## H3 — direct-speech interruption and echo resistance

1. Explicitly enable hands-free mode and start a long TTS reply.
2. Speak directly while TTS is playing; do not press the interrupt button.
3. Confirm playback stops, the new utterance reaches final STT and the replacement turn is dispatched once.
4. Stay silent during another TTS segment and confirm the app does not transcribe its own playback as approval or a new command.
5. Disable hands-free mode and confirm the microphone/device indicator is released.

## H4 — voice task status, pause, resume and retry

1. Start a safe, independently executing Workflow-backed TaskGraph.
2. Ask for current task/status and compare the spoken answer with the visible graph.
3. Speak pause, then confirm no next node starts; speak resume and confirm execution continues.
4. If using retry/rerun, confirm a new attempt is added while prior attempt/history remains visible.
5. Ending voice mode must not implicitly cancel the task.

## H5 — representative file, artifact, canvas, voice and monitor flow

1. Attach a supported file, remove/re-add it before send, and bind it to a real Conversation/Task/Node.
2. Confirm parsing status and bounded preview; for a scanned PDF without text, accept only an explicit OCR-unsupported result.
3. Run a safe task that produces a persisted artifact, preview it, download it and confirm provenance/version/source node.
4. On the Task Canvas, multi-select two nodes, create a visual group, collapse/expand it and run auto-layout.
5. Select and preview an existing provider voice/voice_id and adjust speech speed.
6. Change System Monitoring between grid/free layouts, then reopen the page and confirm persistence and real/empty/unavailable data only.

### Managed Capability Import

7. Create or select one small, non-sensitive Markdown Skill and complete the real lifecycle: Import → Preview → Confirm.
8. Confirm the installed record starts disabled, then explicitly enable it and verify the Skill appears through existing managed Skill discovery/load behavior.
9. Disable it again; if a previous version exists, exercise rollback and confirm the selected version/status is reflected without executing import text as a system instruction.

ZIP/GitHub malicious-input matrices are covered by automated security evidence and do not need to be manually repeated here.

### Memory-to-Skill actual reuse

10. Complete one authorized task with useful, non-sensitive experience and generate a Memory-to-Skill candidate.
11. Review the candidate's Task/Node/Execution source evidence, edit it, validate it and explicitly confirm it.
12. Confirm a managed Skill version is created and visible, then reuse that Skill in a later applicable task. Merely observing a generated `SKILL.md` file is not acceptance evidence.

## H6 — installer, startup, permissions, shutdown and resource observation

1. Verify the installer filename/version and SHA-256 against the technical report.
2. Install into an isolated test directory and start the application normally.
3. Confirm a visible v1 Workspace window, healthy backend `1.0.0-rc.1`, correct first-run setup and no v2 DesktopSpace startup.
4. Exercise one permission/approval flow. Confirm rejection is recorded as the real legal terminal state defined by the existing ApprovalStore, the unauthorized action did not execute, and the corresponding Task/Execution is not incorrectly reported as succeeded.
5. Observe the complete application process group during idle and representative work; do not report one process as total memory.
6. Close normally, confirm the backend exits, restart and confirm tasks/layouts/history recover.
7. Uninstall and record whether user-selected data is retained or removed as documented.

## Evidence record

| Item | Status | Evidence to record |
| --- | --- | --- |
| H1 | HUMAN_PENDING | timestamp, redacted conversation ID, transcript meaning, audible yes/no |
| H2 | HUMAN_PENDING | A/B redacted IDs and observed anchor behavior |
| H3 | HUMAN_PENDING | direct-speech interruption, echo result, device release |
| H4 | HUMAN_PENDING | graph/execution redacted IDs and pause/resume attempt states |
| H5 | HUMAN_PENDING | resource/artifact IDs, downloaded hash, layout, managed import lifecycle and later Skill reuse evidence |
| H6 | HUMAN_PENDING | installer hash, startup/close/restart/uninstall and process-group observation |

Any failure remains `FAILED` or `BLOCKED` with its concrete prerequisite. Only the affected item is repeated after a fix. Formal v1.0 acceptance requires all applicable rows to become `ACCEPTED`.
