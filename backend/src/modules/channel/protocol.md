# External channel protocols

This adapter implements private text messages only. It creates a separate local
conversation per channel and remote peer. Each channel explicitly selects
automatic delivery or per-message local review. New replies in automatic mode
are sent directly and recorded as automatic, without fabricating a manual
approval timestamp. No desktop tools are exposed.

## Feishu

- [Official event subscription configuration](https://open.feishu.cn/document/server-docs/event-subscription-guide/event-subscription-configure-/request-url-configuration-case)
- [Official SDK callback authentication](https://github.com/larksuite/oapi-sdk-python/blob/v2_main/lark_oapi/event/dispatcher_handler.py)
- [Official receive-message event types](https://github.com/larksuite/oapi-sdk-python/blob/v2_main/lark_oapi/api/im/v1/model/p2_im_message_receive_v1.py)
- [Official reply-message API](https://open.feishu.cn/document/server-docs/im-v1/message/reply)

Self-built applications can explicitly select the official WebSocket connection
or the existing HTTP subscription. WebSocket mode discovers the platform endpoint,
decodes protobuf frames, joins event fragments and acknowledges accepted events.
Application heartbeat and reconnect settings follow the official server response.
See the [official SDK WebSocket client](https://github.com/larksuite/node-sdk/blob/main/ws-client/index.ts).

HTTP mode supports URL verification and unencrypted v2 `im.message.receive_v1`
events with the configured Verification Token and App ID. This mode requires a
reachable HTTPS callback; WebSocket mode does not. Both modes require published
bot/message permissions and configured allowed open IDs. Group messages,
encrypted HTTP callbacks and media processing are outside this private-text
scope. API secrets stay in SecretStore; tenant access tokens are obtained at send
time and are not persisted or logged. No connection mode is selected as an
automatic substitute for another mode.

## OneBot 11

- [HTTP POST event signatures](https://github.com/botuniverse/onebot-11/blob/master/communication/http-post.md)
- [HTTP authentication](https://github.com/botuniverse/onebot-11/blob/master/communication/authorization.md)
- [Private-message API](https://github.com/botuniverse/onebot-11/blob/master/api/public.md#send_private_msg-发送私聊消息)

Supports a separately operated OneBot 11 HTTP service, signed reverse-HTTP text
events, `X-Signature: sha1=...` over the exact raw body, self-ID matching,
allowed sender IDs, and bearer-authenticated structured actions. It does not
provide or install a QQ login implementation. QQ group ingress is limited to
configured `allowed_groups`; the channel's `group_trigger` chooses whether the
bot must be mentioned or all text messages are accepted. Member IDs remain
separate from group IDs. Replies use text segments, and only status=ok,
retcode=0, plus a returned platform message ID counts as delivered.

Reverse WebSocket mode exposes `/api/channel-callbacks/:id/ws`, validates
`Authorization: Bearer ...` and `X-Self-ID` before upgrading, and uses the same
connection for events and structured OneBot actions. Each action has an `echo`
correlation value. HTTP and reverse-WebSocket action calls return only the
platform's `data` after an explicit successful acknowledgement. A dropped
connection or timeout after dispatch is reported as unknown delivery, not a safe
opportunity to repeat the request.

## Weixin iLink

[Tencent's protocol](https://github.com/Tencent/openclaw-weixin/blob/874637a0dbc24050f72d1385add4fc6170a2d726/docs/protocol_zh_CN.md)
is implemented for QR authorization, phone confirmation and numeric verification,
long polling, cursor persistence and private-text replies. Numeric codes are
submitted via `get_qrcode_status`'s `verify_code` parameter. Platform-requested QR
node redirects retain the actual returned endpoint. Bot tokens and per-message
`context_token` values stay in SecretStore; the SQLite runtime state stores the
poll cursor. Context credentials are stored before the corresponding message is
accepted, so persistence errors do not leave a deduplicated message without its
reply credentials.

If the allow-list is empty at initial authorization, only the platform-returned
scanning user ID is added. Login completion is separate from enabling reception.
A successful HTTP `sendmessage` response with a JSON object and no nonzero `ret`
confirms platform acceptance; the protobuf default `ret=0` may be omitted. This
matches the [official client and empty-object success test](https://github.com/Tencent/openclaw-weixin/blob/874637a0dbc24050f72d1385add4fc6170a2d726/src/api/api.test.ts#L182-L198).
The submitted `client_id` identifies this acceptance; it is not a read receipt.
Long-poll errors inspect the official `ret` and `errcode` fields; session-expired
code `-14` requires the user to scan again.
Media, mixed messages and groups are recorded as unsupported observations and do
not trigger model generation.

## Reply presets and lifecycle

The three built-in presets can be copied into user presets referencing existing
agent/persona and model profiles. Both preview and inbound generation call the
configured model; preview never sends an external message. Channel generation
keeps the existing tool/memory/knowledge restrictions. Automatic delivery checks
the persisted intake policy, current enabled state, current policy and allow-list
before claiming one send. Manual review applies to the reviewed destination and
body once. Changing policy does not send older reviewable drafts.

Connections and QR tasks use application-owned CancellationTokens. Disabling,
editing, deletion and shutdown cancel and await owned work. Deleting a channel
clears its configuration, channel inbox/session mappings and credential references;
existing local chat history remains. In-flight sends must resolve before deletion.

Protocol sources inspected 2026-10-23. Controlled HTTP and WebSocket tests verify
wire requests, event parsing, acknowledgements, cursors, approval and acceptance
semantics. Real account login and delivery are separate acceptance checks; the
evidence report records whether those were actually observed.
