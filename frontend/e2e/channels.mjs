import assert from "node:assert/strict";
import { join } from "node:path";
import { mkdir, writeFile } from "node:fs/promises";

/** Controlled browser acceptance for channel UI contracts; platform APIs are intercepted. */
export async function channels({ page, frontendPort, evidenceDir }) {
  const origin = `http://127.0.0.1:${frontendPort}`;
  const now = Date.now();
  const providers = [
    { id: "feishu", label: "飞书自建应用", supported: true, protocol: "飞书事件订阅 v2 与官方长连接", docs_url: "https://open.feishu.cn/", limitations: ["仅名单内用户的私聊文本"] },
    { id: "onebot11", label: "QQ · OneBot 11", supported: true, protocol: "OneBot 11 HTTP 与反向 WebSocket", docs_url: "https://github.com/botuniverse/onebot-11", limitations: ["需独立运行兼容 OneBot 11 的服务"] },
    { id: "wechat_ilink", label: "微信 · iLink", supported: true, protocol: "腾讯官方 iLink 登录与轮询", docs_url: "https://github.com/Tencent/openclaw-weixin", limitations: ["扫码登录后仅处理允许联系人的私聊文本"] },
  ];
  const presets = [
    { id: "preset-kindness", name: "昔涟 · 温柔日常", agent_id: null, model_profile_id: null, instructions: "语气自然、温柔，先回应对方的重点。", builtin: true, created_at: now, updated_at: now },
    { id: "preset-work", name: "简洁工作助手", agent_id: null, model_profile_id: null, instructions: "清楚、简洁地回答工作问题。", builtin: true, created_at: now, updated_at: now },
    { id: "preset-study", name: "学习答疑", agent_id: null, model_profile_id: null, instructions: "分步骤解释学习问题。", builtin: true, created_at: now, updated_at: now },
  ];
  const channel = (id, name, provider, connection_mode, preset_id = "preset-kindness", reply_mode = "manual") => ({
    id, name, provider, enabled: true, app_id: provider === "feishu" ? "app-fixture" : null,
    api_base_url: null, self_id: null, allowed_senders: ["peer-fixture"], connection_mode, preset_id, reply_mode,
    status: "connected", last_received_at: now, last_event: null, last_error: null,
    created_at: now, updated_at: now, callback_path: `/api/channel-callbacks/${id}`, credentials_configured: true,
  });
  const channelsById = new Map([["channel-feishu", channel("channel-feishu", "工作飞书", "feishu", "websocket")]]);
  const messagesByChannel = new Map([["channel-feishu", [{
    id: "message-fixture", channel_id: "channel-feishu", conversation_id: "conversation-fixture",
    event_id: "event-fixture", external_message_id: "external-fixture", sender_id: "测试联系人",
    content: "这周的计划可以帮我整理一下吗？", draft_reply: "可以，我先把计划整理成清晰的几项。",
    status: "awaiting_send", send_mode: null, error: null, reply_message_id: null, approved_at: null, created_at: now, updated_at: now,
  }]]]);
  let login = null;
  let customPreset = null;
  const approvedSends = [];
  const qr = `data:image/svg+xml,${encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="220" height="220"><rect width="220" height="220" fill="white"/><path d="M25 25h50v50H25zm120 0h50v50h-50zM25 145h50v50H25zm88-78h18v18h-18zm0 44h18v18h-18zm31 31h31v31h-31z" fill="black"/></svg>')}`;

  const fulfill = (route, body, status = 200) => route.fulfill({ status, contentType: "application/json", body: JSON.stringify(body) });
  await page.route("**/api/**", async route => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();

    if (path === "/api/channels/providers" && method === "GET") return fulfill(route, { providers });
    if (path === "/api/channels" && method === "GET") return fulfill(route, { channels: [...channelsById.values()] });
    if (path === "/api/channels" && method === "POST") {
      const input = request.postDataJSON();
      const created = { ...channel("channel-wechat", input.name, input.provider, input.connection_mode, input.preset_id, input.reply_mode), status: "awaiting_login", credentials_configured: false, allowed_senders: input.allowed_senders };
      channelsById.set(created.id, created);
      messagesByChannel.set(created.id, []);
      return fulfill(route, { channel: created });
    }
    if (path === "/api/channels/presets" && method === "GET") return fulfill(route, { presets: customPreset ? [...presets, customPreset] : presets });
    if (path === "/api/channels/presets" && method === "POST") {
      customPreset = { id: "preset-custom", ...request.postDataJSON(), builtin: false, created_at: now, updated_at: now };
      return fulfill(route, { preset: customPreset });
    }
    if (/^\/api\/channels\/presets\/[^/]+\/preview$/.test(path) && method === "POST") return fulfill(route, { reply: "这是一条本地试用回复，不会发送到外部渠道。", conversation_id: "preview-conversation" });
    if (path === "/api/agents" && method === "GET") return fulfill(route, { agents: [{ id: "agent-default", name: "昔涟", description: "默认角色", user_prompt: "", persona_enabled: true, persona: {}, allowed_tools: [], model: null, capabilities: [], max_iterations: 8, enabled: true, source: "builtin" }] });
    if (path === "/api/llm/models" && method === "GET") return fulfill(route, [{ id: "model-fixture", provider: "local", label: "当前聊天模型", model: "fixture", base_url: "", api_format: "openai", api_key_configured: true, api_key_source: "local", api_key_env: "", temperature: 0.4, max_tokens: 2048, invoke_timeout_ms: 30000, active: true, created_at: now, updated_at: now }]);

    const channelPath = path.match(/^\/api\/channels\/([^/]+)(?:\/(.*))?$/);
    if (channelPath) {
      const channelId = decodeURIComponent(channelPath[1]);
      const rest = channelPath[2] ?? "";
      const current = channelsById.get(channelId);
      if (rest === "messages" && method === "GET") return fulfill(route, { messages: messagesByChannel.get(channelId) ?? [] });
      if (rest === "check" && method === "POST" && current) return fulfill(route, { checks: [{ label: "账号凭据", ok: true, detail: "平台返回了有效的账号信息。" }, { label: "连接", ok: true, detail: "渠道运行连接已建立。" }], channel: current });
      if (rest === "enabled" && method === "POST" && current) {
        const updated = { ...current, enabled: request.postDataJSON().enabled };
        channelsById.set(channelId, updated);
        return fulfill(route, { channel: updated });
      }
      if (rest === "reply-mode" && method === "POST" && current) {
        const updated = { ...current, reply_mode: request.postDataJSON().reply_mode };
        channelsById.set(channelId, updated);
        return fulfill(route, { channel: updated });
      }
      if (rest === "login" && method === "POST") {
        login = { status: "scanning", qr_code_data_url: qr, expires_at: now + 300000, bot_id: null, error: null };
        return fulfill(route, { login });
      }
      if (rest === "login" && method === "GET") {
        login = { status: "need_verifycode", qr_code_data_url: null, expires_at: now + 300000, bot_id: null, error: null };
        return fulfill(route, { login });
      }
      if (rest === "login/verify" && method === "POST") {
        assert.match(request.postDataJSON().code, /^\d+$/);
        login = { status: "confirmed", qr_code_data_url: null, expires_at: null, bot_id: "wechat-fixture", error: null };
        const updated = { ...current, status: "connected", allowed_senders: ["wechat-fixture"], credentials_configured: true };
        channelsById.set(channelId, updated);
        return fulfill(route, { login });
      }
      if (rest === "login" && method === "DELETE") {
        login = { status: "cancelled", qr_code_data_url: null, expires_at: null, bot_id: null, error: null };
        return fulfill(route, { login });
      }
      const messagePath = rest.match(/^messages\/([^/]+)\/(approve-send|regenerate|dismiss)$/);
      if (messagePath) {
        const messageId = decodeURIComponent(messagePath[1]);
        const action = messagePath[2];
        const rows = messagesByChannel.get(channelId) ?? [];
        const old = rows.find(item => item.id === messageId);
        if (!old) return fulfill(route, { error: "消息不存在" }, 404);
        let updated;
        if (action === "approve-send") {
          const input = request.postDataJSON();
          assert.equal(input.approved, true);
          approvedSends.push(input.text);
          updated = { ...old, draft_reply: input.text, status: "sent", send_mode: "manual", reply_message_id: "fixture-accepted", approved_at: now, updated_at: now };
        } else if (action === "regenerate") {
          updated = { ...old, status: "running", error: null, updated_at: now };
        } else {
          updated = { ...old, status: "dismissed", updated_at: now };
        }
        messagesByChannel.set(channelId, rows.map(item => item.id === messageId ? updated : item));
        return fulfill(route, { message: updated });
      }
      if (!rest && method === "DELETE") {
        channelsById.delete(channelId);
        messagesByChannel.delete(channelId);
        return fulfill(route, { deleted: true });
      }
    }
    return route.continue();
  });

  if (evidenceDir) await mkdir(evidenceDir, { recursive: true });
  const capture = async name => {
    if (evidenceDir) await page.screenshot({ path: join(evidenceDir, `${name}.png`), fullPage: false, animations: "disabled" });
  };
  const checks = [];
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(`${origin}/channels`);
  await page.getByRole("heading", { name: "消息渠道", exact: true }).waitFor();
  await page.getByRole("heading", { name: "收信与回复", exact: true }).waitFor();
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await capture("channels-inbox");

  const oldDraft = messagesByChannel.get("channel-feishu")[0];
  await page.getByRole("button", { name: "开启自动回复", exact: true }).click();
  await page.getByRole("button", { name: "改为审阅发送", exact: true }).waitFor();
  assert.equal(channelsById.get("channel-feishu").reply_mode, "automatic");
  assert.equal(messagesByChannel.get("channel-feishu")[0].status, oldDraft.status);
  assert.equal(approvedSends.length, 0);
  await page.getByRole("button", { name: "改为审阅发送", exact: true }).click();
  await page.getByRole("button", { name: "开启自动回复", exact: true }).waitFor();
  assert.equal(channelsById.get("channel-feishu").reply_mode, "manual");
  checks.push("reply_policy_changes_update_from_server_without_sending_existing_drafts");

  await page.getByRole("button", { name: /测试联系人/ }).click();
  await page.getByRole("textbox", { name: "回复正文", exact: true }).fill("我会把本周计划整理成清晰的几项。\n稍后发你确认。");
  await page.getByRole("button", { name: "审阅并批准发送", exact: true }).click();
  const review = page.getByRole("dialog", { name: "确认发送外部消息", exact: true });
  await review.waitFor();
  await review.getByText("工作飞书", { exact: false }).waitFor();
  await review.getByText("测试联系人", { exact: false }).waitFor();
  await review.getByText("稍后发你确认。", { exact: false }).waitFor();
  await capture("channels-send-review");
  await review.getByRole("button", { name: "批准并发送此消息", exact: true }).click();
  await page.getByText("发送确认标识：fixture-accepted", { exact: true }).waitFor();
  assert.equal(approvedSends.length, 1);
  checks.push("editable_reply_requires_destination_review_and_one_approval");

  const message = messagesByChannel.get("channel-feishu")[0];
  messagesByChannel.set("channel-feishu", [{ ...message, status: "delivery_unknown", reply_message_id: null }]);
  await page.getByRole("button", { name: "刷新消息", exact: true }).click();
  await page.getByRole("status").filter({ hasText: "未取得平台送达确认" }).waitFor();
  assert.equal(await page.getByRole("button", { name: "审阅并批准发送", exact: true }).count(), 0);
  assert.equal(approvedSends.length, 1);
  checks.push("unknown_delivery_is_visible_and_has_no_resend_action");

  await page.getByRole("button", { name: "回复预设", exact: true }).click();
  await page.getByRole("button", { name: /昔涟 · 温柔日常/ }).click();
  await page.getByRole("button", { name: "复制为自定义", exact: true }).click();
  await page.getByRole("textbox", { name: "预设名称", exact: true }).fill("本地验收预设");
  await page.getByRole("button", { name: "保存预设", exact: true }).click();
  await page.getByRole("heading", { name: "本地验收预设", exact: true }).waitFor();
  await page.getByRole("textbox", { name: "试用消息", exact: true }).fill("请问今天的安排？");
  await page.getByRole("button", { name: "运行本地试用", exact: true }).click();
  await page.getByText("这是一条本地试用回复，不会发送到外部渠道。", { exact: true }).waitFor();
  await capture("channels-preset-preview");
  checks.push("builtin_preset_copies_to_editable_custom_and_preview_stays_local");

  await page.getByRole("button", { name: "返回渠道", exact: true }).click();
  await page.getByRole("button", { name: "添加渠道", exact: true }).click();
  const wizard = page.getByRole("dialog", { name: "添加消息渠道", exact: true });
  await wizard.waitFor();
  await wizard.getByRole("combobox", { name: "平台模板", exact: true }).selectOption("wechat_ilink");
  await wizard.getByRole("textbox", { name: "渠道名称", exact: true }).fill("微信桌面接入");
  assert.equal(await wizard.getByRole("combobox", { name: "连接方式", exact: true }).inputValue(), "polling");
  await wizard.getByText(/扫码绑定的微信用户 ID 作为唯一允许联系人/).waitFor();
  await wizard.getByRole("combobox", { name: "渠道回复预设", exact: true }).selectOption("preset-custom");
  await capture("channels-wechat-wizard");
  const channelCreateResponse = page.waitForResponse(response => response.url().endsWith("/api/channels") && response.request().method() === "POST");
  await wizard.getByRole("button", { name: "保存配置", exact: true }).click();
  const wechatInput = await channelCreateResponse;
  assert.equal(wechatInput.status(), 200);
  const postedChannel = await wechatInput.request().postDataJSON();
  assert.equal(postedChannel.connection_mode, "polling");
  assert.equal(postedChannel.preset_id, "preset-custom");
  assert.equal(postedChannel.reply_mode, "automatic");
  assert.deepEqual(postedChannel.allowed_senders, []);
  await page.getByRole("heading", { name: "微信桌面接入", exact: true }).waitFor();
  await page.getByRole("button", { name: "开始扫码", exact: true }).click();
  await page.getByRole("img", { name: "微信 iLink 登录二维码", exact: true }).waitFor();
  await page.getByLabel("手机显示的数字验证码", { exact: true }).waitFor();
  await page.getByLabel("手机显示的数字验证码", { exact: true }).fill("52831");
  await page.getByRole("button", { name: "验证并继续", exact: true }).click();
  await page.getByText("登录状态：微信已确认登录", { exact: true }).waitFor();
  await page.getByRole("button", { name: "完成", exact: true }).click();
  assert.deepEqual(channelsById.get("channel-wechat").allowed_senders, ["wechat-fixture"]);
  checks.push("wechat_polling_template_creates_qr_login_and_numeric_verification_flow");

  await page.getByRole("button", { name: "检查连接", exact: true }).click();
  await page.getByRole("heading", { name: "连接检查", exact: true }).waitFor();
  await page.getByText("平台返回了有效的账号信息。", { exact: true }).waitFor();
  await page.getByRole("button", { name: /删除 微信桌面接入/ }).click();
  const deletion = page.getByRole("dialog", { name: "删除消息渠道", exact: true });
  await deletion.getByText("收件箱记录和会话关联", { exact: false }).waitFor();
  await deletion.getByText("已有本地对话与聊天消息会保留", { exact: false }).waitFor();
  await deletion.getByRole("button", { name: "取消", exact: true }).click();
  checks.push("connection_checks_and_delete_confirmation_explain_actual_scope");

  await page.setViewportSize({ width: 760, height: 900 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await capture("channels-mobile");
  await page.setViewportSize({ width: 1440, height: 900 });
  const result = { status: "passed", checks, evidence: "All channel/model requests above were intercepted by controlled browser fixtures; no real platform account or outbound delivery was used." };
  if (evidenceDir) await writeFile(join(evidenceDir, "channels.json"), JSON.stringify(result, null, 2));
  return result;
}
