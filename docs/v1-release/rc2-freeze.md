# YiLianQianYan v1.0.0-rc.2 freeze

Status: `FROZEN` for the verified source and installer candidate.

## Frozen identity

- Baseline: `38e135437a9d211282420d7c6b981f70d7d6f3ff`.
- Functional application checkpoint: `81afbed6f2cdf09b0590064a4293ae47472ad1bd`.
- Technical/packaging checkpoint: `ac5745553cd6304432785f1ad796e1c04e79582b`.
- Version: `1.0.0-rc.2`.
- Installer: `忆涟千言_1.0.0-rc.2_x64-setup.exe`.
- Size: `8,838,102` bytes.
- SHA-256: `3B4FA8A73C5A155CCC447B81B094D3A268CDFBAEA29360D01B478C2FA37BE406`.

The `1.0.0-rc.1` installer, checksum and evidence are historical artifacts and must not be overwritten.

## Freeze boundary

Any change to application source, dependencies, manifests, Tauri configuration or packaging input invalidates this rc.2 freeze and requires a new candidate version plus a proportionate gate. Documentation-only handoff metadata may follow if it clearly records the unchanged functional/packaging checkpoints.

Do not start Identity, Cloud Gateway, Cloud Harness, Web Surface, v2 or Gate 4 work from this freeze task.

## Remaining acceptance

- Packaged GUI automation passed after the independently running YiLian process released port 9420 naturally: visible window, matching health/version, screenshot-content check, normal close, port release and isolated uninstall all succeeded.
- Remote CI remains `BLOCKED_BY_WORKFLOW_SCOPE`.
- Real microphone, cloud Provider, audible TTS, acoustic echo behavior, direct spoken barge-in and installer usability remain `HUMAN_PENDING` under `v1-h-checklist.md`.
- Formal v1.0.0 remains `NOT RELEASED`.

