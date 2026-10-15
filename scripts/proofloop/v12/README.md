# ProofLoop V12 evaluation runner

This directory owns the V12 real-model runner, independent byte judge, and experiment summarizer. The frozen task source remains `backend/tests/fixtures/proofloop/benchmark-tasks.json`; its SHA-256 is checked against `manifest.json` before any stage starts.

The runner does not provide model responses, capabilities, or task outputs. It calls each dedicated backend's real HTTP API, uses the configured active and verified model, starts the compiled task graph, drives the product's approval SSE route as `automated_test_driver`, materializes each deliverable from its exact current node execution, downloads the artifact bytes, and reads the full paginated security audit. A controlled fixture, an unavailable capability, or a missing environment value never becomes a passing run.

## Required local environment

Create local files outside the repository and set these variables in the shell that will run the CLI:

| Variable | Required value |
| --- | --- |
| `YILIAN_V12_GROUP_INSTANCES_FILE` | Absolute path to the local JSON file described below. |
| `YILIAN_V12_TASK_BINDINGS` | Absolute path to the actual logical-capability to registry-ID mapping described below. |
| `YILIAN_V12_ISOLATED_PROFILE` | Absolute existing directory that contains three distinct dedicated V12 backend profile directories. |
| `YILIAN_V12_BUDGET_AUTHORIZED` | `1` only after the operator has authorized provider spend for the planned stage. Required for preflight and stage execution. |
| `YILIAN_V12_SIDE_EFFECTS_AUTHORIZED` | `1` only after the operator has authorized the real high-risk task tools to run in the dedicated test environments. Required for preflight and stage execution. |

Each group instance entry contains:

```json
{
  "api_base": "https://the-actual-dedicated-backend.example",
  "control_session_env": "YILIAN_V12_B0_CONTROL_SESSION",
  "profile_dir": "V12-B0",
  "workspace_root": "workspaces",
  "model_id": "the-actual-active-model-profile-id",
  "binary_sha256": "the-actual-64-character-backend-binary-sha256",
  "binary_source_commit": "the-source-commit-mapped-to-that-binary",
  "model_pricing": {
    "source_url": "https://the-provider.example/pricing",
    "observed_at_ms": 1792108800000,
    "currency": "USD",
    "input_per_million": 0,
    "output_per_million": 0
  }
}
```

The file root is `{ "schema_version": 1, "groups": { "V12-B0": {}, "V12-B1": {}, "V12-B2": {} } }`. The `profile_dir` values must be distinct directories under `YILIAN_V12_ISOLATED_PROFILE`; the three `api_base` values must also be distinct. Each endpoint must point to its corresponding dedicated backend and database/profile, with no unrelated application traffic during the run. `workspace_root` is resolved under that profile and the runner creates a unique workspace directory for each scheduled slot. The backend API does not expose its active DB/profile path; this correspondence is an operator-provided environment fact.

The JSON fragment is a schema example. Replace the binary hash, source commit, timestamp, and prices with observed values. The displayed `0` prices are placeholders; leave a zero only when the cited provider agreement actually sets that price.

Set each `control_session_env` name to the environment variable holding that group's control-session token. The runner reads the token only in memory and never writes it to evidence. Do not put tokens or API keys in the instance file. Group model profiles must have the same provider, model identifier, format, temperature, token limit, timeout, and pricing source. B0/B1 must report `ai_repair_enabled=false`; B2 must report `true`. The runner reads redacted settings and never writes them back.

`binary_sha256` and `binary_source_commit` must describe the deployed executable and the exact source mapped to it. If either is absent or the source commit does not match the local runner source, the experiment can still be summarized but is not publishable as G4. The current health API exposes package version but not backend source commit, so `server_commit_verified` remains false and the report says so.

The `model_pricing` values must come from the actual provider contract or published price source. The report calls the resulting amount an estimate; the backend usage API does not expose provider invoices. Preflight rejects missing or unequal group pricing. It also rejects model/provider identifiers that look like fixtures, mocks, stubs, fakes, or loopback responders. An active model profile must be verified and have a configured secret source.

The task bindings file contains actual registry IDs and hashes for every logical capability referenced in the frozen task set. Keep it outside the repository; each evidence directory receives only its sanitized ID/hash projection:

```json
{
  "schema_version": 1,
  "bindings": {
    "mcp__doc_fetcher": {
      "capability_id": "actual-capability-registry-id",
      "schema_sha256": "sha256-of-canonical-input-and-output-schema"
    }
  }
}
```

The digest is SHA-256 over canonical JSON for `{ "input_schema": ..., "output_schema": ... }`; object keys are sorted recursively and array order is preserved. Preflight compares every group against the current registry, requires identical registry and skill-corpus hashes, and fails if any frozen logical capability is missing, unavailable, disabled, or changed. Use real capabilities backed by the isolated test services configured for this benchmark.

## Commands

The source worktree must be committed and clean before preflight. Preflight performs only health, settings, model, capability, skill, and route reads plus one invalid-body route probe. It does not call the model or create product records.

```powershell
node scripts/proofloop/v12/run.mjs preflight --manifest scripts/proofloop/v12/manifest.json
```

Preflight returns `PREFLIGHT_READY` only if all three groups are configured, all group settings match the manifest, the task and schemas are frozen, the budget and side-effect authorizations are set, the model profiles are real/verified, and the output/profile roots are available. Missing inputs return `BLOCKED_ENV` with variable names and no PASS rows.

Pilot schedules one original task from each frozen category in all three groups: 9 slots, saved randomized group order, and the manifest seed. Review pilot time, usage, and task/config behavior before starting the main run.

```powershell
node scripts/proofloop/v12/run.mjs pilot --manifest scripts/proofloop/v12/manifest.json --output target/v12-evidence/pilot --authorize-model-spend --authorize-side-effects
node scripts/proofloop/v12/verify.mjs --input target/v12-evidence/pilot
node scripts/proofloop/v12/summarize.mjs --input target/v12-evidence/pilot
```

The main schedule is 30 tasks × 3 repeats × 3 groups = 270 slots. Groups are shuffled within each task/repeat block. The runner does not start the main stage unless both authorization flags and both authorization environment values are present.

```powershell
node scripts/proofloop/v12/run.mjs main --manifest scripts/proofloop/v12/manifest.json --output target/v12-evidence/main --authorize-model-spend --authorize-side-effects
node scripts/proofloop/v12/verify.mjs --input target/v12-evidence/main
node scripts/proofloop/v12/summarize.mjs --input target/v12-evidence/main
node --test scripts/proofloop/v12/summarize.test.mjs
```

The output directory must not already exist. It contains the frozen manifest and task bindings, preflight snapshot, randomized schedule, terminal `runs.jsonl`, separate `events.jsonl`, per-run request/evidence files, redacted actual Task Harness execution snapshots, original downloaded artifacts for each attempt, full audit pages and pagination receipts, independently generated `judged-runs.jsonl`, and summary/report outputs. Interrupted or unavailable slots are recorded as `BLOCKED_ENV`/`NOT_TESTED` or remain visibly absent from the schedule; they do not enter the completion numerator.

## Evidence boundary

The judge reads the copied task specification and downloaded artifact bytes, validates declared format, size and SHA-256, required sections, source URIs, report/snapshot identity, deliverable provenance, and execution-attempt IDs. It independently recomputes illegal execution counts from the selected complete audit events. Missing audit pages or an uncorrelated successful execution leave the count `null`, which blocks acceptance. The product's own `status=pass` is retained only as a claim and cannot set `judge_pass`.

Only Markdown and JSON are in the frozen 30-task set. Other file formats fail closed until this directory includes a real independent parser. Product results requiring human review remain unverified; this runner never submits synthetic human review. The task set contains no such required criterion today. Structural and source checks do not establish professional-content quality or human satisfaction.

Cost, active time, model-call, and peak-memory figures remain unknown when the product APIs do not expose them per run. In particular, peak backend RSS is recorded as `null`. When AI repair is attempted and its API response does not expose a call count, `model_calls` is `null`; the recorded usage tokens and the unknown count are reported separately. A positive safety violation or any product false-pass blocks acceptance. A missing metric alone remains an explicitly reported unknown unless the evidence is needed to prove security or a passing artifact.
