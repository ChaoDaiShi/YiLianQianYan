# Workbench UX U2

Baseline: cd5100eb153ce0a2415b5db5215581b6f018f895.
Branch: feat/v1-workbench-ux-u2. Worktree: YiLian-v1-workbench-u2.
User authorized implementation, isolated worktree, local commits and feature push.
Inline execution only, no subagents. No extra design approval checkpoint.

## Diagnosis and choice

Settings exposes runtime fallback, embedding, multi-model profiles, model tree and
usage with similar visual weight. Tests address the active runtime profile, which
can override the default model fields; the redesign must disclose that precedence.
Chat uses layered translucent borders and a decorative home; Capability starts
with a technical import form before the real inventory. Shared Panel/PageHeader
use glass and shadow even for ordinary work surfaces.

Options: global recoloring alone does not simplify configuration; a new setup
wizard duplicates state and risks Secret behavior. Chosen: progressive disclosure
inside existing components, small presentation helpers, a shared quiet surface
layer, and explicit real-state labels. Existing APIs and credential handlers stay.

## Design contracts

- Work surface: opaque theme surface; elevated panel: light border and no blur;
  floating control: 12px radius, light shadow. Canvas styles/layout remain frozen.
- U1 tokens, typography, 1180px form maximum and voice safe-area remain authoritative.
- Basic model: provider preset, write-only API key, model and truthful connection
  status. Advanced: URL, environment variable, timeout, sampling and embedding.
  Presets fill existing fields only; Custom/Compatible preserve existing BYOK data.
- Existing multi-model profiles/tree/usage are retained under disclosure. Active
  profile precedence is stated explicitly. Save and Test are one visible action
  group but remain separate: the current verify endpoint tests runtime state, not
  an arbitrary unsaved draft. Dirty input must not display a fresh success claim.
- Status: unconfigured; saved but untested; connected after a real successful test;
  unavailable after failure. No persisted availability claim inferred from save.
- Voice is optional. STT/TTS main service/model/key stays visible, provider-specific
  fields move to advanced. No invented enable flag or runtime changes.
- Chat keeps content central, quiet conversation rows, one composer action cluster,
  collapsed execution details with real status/time/summary, and distinct artifacts.
- Capability inventory precedes import management. Cards show actual name,
  description, source, status and permissions; detail shows source/version when
  present. No invented availability, updates, approval status or provenance.
- Memory/Knowledge/System/Nav and global voice receive presentation-only changes.
  Dialog remains interrupting confirmation; drawer auxiliary information;
  floating controls retain explicit triggers. Reduced-motion behavior remains.

## Plan and verification

- [ ] W0: baseline screenshots/styles/geometry for Chat, Settings, Capability,
  System at 1280/1920; commit this design and capture runner.
- [ ] W1: Settings disclosure/presets/status, preserve secret handlers and APIs;
  focused tests for presets/status and existing credential contracts.
- [ ] W2: Chat surfaces, sidebar, composer, execution summaries and artifacts.
- [ ] W3: capability cards/detail and managed import disclosure; preserve source actions.
- [ ] W4/W5: system/knowledge/memory/nav and voice visual consistency only.
- [ ] W6: full frontend tests/build/architecture checks; core + frozen Canvas
  Stability + Studio + U2 real backend/system Edge E2E. Four desktop sizes and
  dark Chat/Settings/Capability/Canvas. Secret tests use only isolated disposable
  fixtures; no real cloud calls, imported unknown code or real user secrets.
- [ ] W7: report/manifest/before-after artifacts in ignored target/workbench-u2,
  protected-path diff, local commits and authorized feature branch push. Stop.

If a backend change is required, stop with WORKBENCH_BACKEND_CHANGE_REQUIRED.
Backend/Cargo/DB/migration/REST/Secret semantics/Canvas state and persistence/v2
are frozen. No dependencies or remote capability installation are planned.
