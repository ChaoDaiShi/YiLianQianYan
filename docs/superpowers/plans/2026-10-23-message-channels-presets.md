# Message Channels and Presets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox syntax for tracking.

**Goal:** Deliver the approved Figma channel workspace with actual Feishu, OneBot 11 and WeChat iLink private text messaging, persisted reply presets, local preview and selectable automatic replies or per-message send approval.

**Architecture:** Extend existing Rust channel modules, SQLite payloads, SecretStore and ChatRuntime. One application-owned ChannelRuntime controls protocol tasks; channel presets reference existing agents and model profiles. React consumes the same channel API and theme tokens.

**Tech Stack:** Rust/Axum/Tokio/SQLite, React/TypeScript/Tauri, existing reqwest and futures, focused WebSocket/protobuf/QR dependencies as required by the official protocols.

## Global Constraints

- Preserve the current branch and all unrelated dirty work; do not commit or push without a request.
- No unrequested defensive development, automatic alternate protocols, silent repairs or architectural migration.
- Private text messages from configured contacts; apply the channel reply policy and preserve existing external-conversation tool restrictions. The latest user request explicitly authorizes automatic replies.
- Report configured, connected, receiving and platform-confirmed sending as separate observations.
- User approved Figma: https://www.figma.com/design/tMmQ64RPAxtShebBwmzOdw .
- Main agent alone owns public types, existing API/DB files, AppServer/bootstrap/lifecycle and Cargo dependency edits.

## Frozen interfaces

```rust
// Additive fields on Channel and ChannelInput:
pub connection_mode: String; // http (existing configs), websocket (Feishu), reverse_websocket (OneBot), polling (WeChat)
pub preset_id: Option<String>;
// Channel status additionally supports connecting, connected, awaiting_login.
// Internal InboundMessage gains context_token: Option<String>; never serialize this to the frontend.

pub struct ChannelPreset {
    pub id: String, pub name: String, pub agent_id: Option<String>,
    pub model_profile_id: Option<String>, pub instructions: String,
    pub builtin: bool, pub created_at: i64, pub updated_at: i64,
}
pub struct ChannelLogin {
    pub status: String, pub qr_code_data_url: Option<String>,
    pub expires_at: Option<i64>, pub bot_id: Option<String>, pub error: Option<String>,
}
// Shared receive boundary, implemented by the integrator:
pub async fn receive(server: Arc<AppServer>, channel: &Channel, inbound: InboundMessage) -> Result<bool, String>;
// DB status updater, implemented by the integrator:
pub fn set_channel_status(&self, id: &str, status: &str, error: Option<&str>) -> Result<(), String>;
// AppServer field:
pub channel_runtime: crate::modules::channel::runtime::ChannelRuntime;
```

### Task 1: WeChat iLink protocol and login (Luna / xhigh)

**Owns:** new `backend/src/modules/channel/wechat.rs` and its own nested files/tests only.

**Consumes:** frozen Channel/ChannelLogin/InboundMessage, SecretStore, `receive`, `set_channel_status` and ChannelRuntime sync.

**Produces:** `WechatLoginManager: Default` with async `start(server: Arc<AppServer>, id: &str) -> Result<ChannelLogin,String>`, `status(server,id) -> Result<ChannelLogin,String>`, `cancel(id: &str)`; `run(server, channel, cancel) -> Result<(),String>`; `send_reply(channel,message,text,store,dispatched) -> Result<String,String>`.

- [x] Read and pin Tencent official protocol and Cyrene QR/polling sources.
- [x] Add focused parsing/login-state tests and observe expected failures before implementation.
- [x] Implement real QR login, credentials in SecretStore, persistent cursor, contextual private-text replies and cancellation.
- [x] Test protocol parsers and request/response behavior with controlled endpoints; report real-account evidence separately.

### Task 2: Feishu/OneBot transports and lifecycle (Luna / xhigh)

**Owns:** new `backend/src/modules/channel/runtime.rs`, `feishu_ws.rs`, `onebot_ws.rs` and their nested tests only.

**Consumes:** frozen receive/status interfaces and `wechat::run`; existing Feishu/OneBot event parsers and HTTP reply implementation.

**Produces:** `ChannelRuntime: Default` with `pub logins: WechatLoginManager`, async `sync(&self, server: Arc<AppServer>, channel: &Channel) -> Result<(),String>`, `stop(&self,id)`, `shutdown(&self)`, `send_onebot(&self,channel,message,text,dispatched) -> Result<String,String>`; `start_channels(server: Arc<AppServer>)`; an Axum reverse WebSocket handler exported by `onebot_ws`.

- [x] Verify official Feishu WS frame schema, endpoint and heartbeat/event acknowledgement protocol.
- [x] Add frame/event and OneBot echo tests; verify they fail on absent behavior.
- [x] Implement task ownership, cancellation, status reporting, Feishu WS and authenticated OneBot reverse WS.
- [x] Verify controlled sockets receive/acknowledge events and correlate send results; no protocol fallback.

### Task 3: Presets, storage and integration (primary agent)

**Owns:** existing channel types/module/API/DB, new preset module/API, Cargo, application lifecycle integration and focused integration tests.

- [x] Persist three built-in presets and user CRUD using the existing SQLite migration system.
- [x] Validate referenced agent/model IDs, apply preset instructions and model to actual channel generation, retain external-session restrictions.
- [x] Implement preview using the real configured ChatRuntime, with no external send.
- [x] Wire protocol lifecycle, login endpoints, channel delete/check, explicit regeneration/dismissal and per-message context secrets.
- [x] Make accepted event insertion and received status one transaction; serialize per-peer generation without losing queued messages.
- [x] Test preset CRUD/reference validation, non-duplicate receive, send approval, delete and runtime cancellation.

### Task 4: Figma frontend (Luna / xhigh)

**Owns:** `frontend/src/features/channels/**`, `frontend/src/api/channels.ts`, new `frontend/e2e/channels.mjs`.

**API contracts:** presets use `/api/channels/presets` GET/POST, `/:id` PUT/DELETE, `/:id/preview` POST `{message}` → `{reply,conversation_id}`; login uses `/api/channels/:id/login` POST/GET/DELETE → `{login:ChannelLogin}`; check POST `/:id/check` → `{checks:[{label,ok,detail}],channel}`; delete DELETE `/:id`; regenerate POST `/:id/messages/:message/regenerate`; dismiss POST `/:id/messages/:message/dismiss`. Existing endpoints unchanged.

- [x] Implement three Figma surfaces with existing PageShell, Button, Modal and theme variables.
- [x] Add explicit platform templates, connection modes, QR state polling, connection checks and config deletion.
- [x] Add preset CRUD/duplicate/model-agent selection/real preview, bind preset to channels.
- [x] Add visible-page inbox refresh, review/edit/send flow and error/loading states; never substitute failures with empty data.
- [x] Verify targeted UI behavior, production build, desktop rendering and responsive layout.

### Task 5: Integrated verification (primary agent plus Luna review)

- [x] Review changed files against both approved spec and frozen interfaces; fix actionable findings.
- [x] Run scoped rustfmt, `cargo fmt --check`, `cargo check`, channel tests and frontend production build.
- [x] Run controlled protocol/API/browser checks and record machine-readable evidence under `artifacts/message-channels-implementation/`.
- [x] Refresh/relaunch the actual desktop when needed and verify its loaded version and UI behavior.
- [x] Report account authorization or external delivery as pending if not actually observed. Preserve user settings and remove only test-owned fixtures.

### Task 6: User-requested automatic replies (latest steering, 2026-10-23)

The user requested automatic replies after completing real WeChat login, inbound,
model generation and manual send/phone receipt. This supersedes the earlier
per-message-review-only requirement for channels explicitly set to automatic.

**Frozen additions:** `Channel.reply_mode` and `ChannelInput.reply_mode` are
`"automatic" | "manual"`; existing persisted channels without this field retain
manual mode. Newly configured channels select automatic in the UI. The current
user-authorized WeChat channel will be explicitly switched to automatic.
`POST /api/channels/:id/reply-mode {reply_mode}` returns `{channel}`.
`ChannelMessage.send_mode` is nullable `"automatic" | "manual"`; automatic sending
does not populate `approved_at`. SQLite migration 1060 adds the message column.

- [x] Add failing automatic-claim and integration tests before implementation.
- [x] Share one protocol dispatch/result function between manual and automatic delivery.
- [x] At the end of each new model generation, re-read the enabled channel, allow-list and reply policy under the existing management gate before claiming a single automatic send.
- [x] Leave previously pending drafts unchanged on mode switches. Keep unknown delivery non-repeatable and expose platform failures.
- [x] Add frontend policy selection and a direct per-channel switch, show automatic/manual provenance, and update model prompt and documentation.
- [x] Run relevant Rust/frontend checks, set the user's actual WeChat channel to automatic in the native desktop, and verify a fresh phone message receives a reply without desktop approval.
