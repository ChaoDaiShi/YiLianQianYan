import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";

/** Controlled browser acceptance for the message workspace and robot APIs. */
export async function channelsRobot({ page, frontendPort, evidenceDir }) {
  const origin = `http://127.0.0.1:${frontendPort}`;
  const now = Date.now();
  const providers = [
    { id: "onebot11", label: "QQ · OneBot 11", supported: true, protocol: "OneBot 11 HTTP 与反向 WebSocket", docs_url: "https://onebot.dev/", limitations: ["以所连接服务实际支持的接口为准"] },
    { id: "wechat_onebot", label: "微信 · Akasha-OneBot", supported: true, protocol: "Akasha-WeChat 反向 WebSocket", docs_url: "https://github.com/alingalingling/Akasha-WeChat", limitations: ["独立 OneBot 适配器；不提供 QQ 式通讯录与历史探索"] },
    { id: "wechat_ilink", label: "微信 · iLink", supported: true, protocol: "微信 iLink 登录与轮询", docs_url: "https://github.com/Tencent/openclaw-weixin", limitations: ["通过微信官方扫码流程登录，不提供 QQ 式通讯录和历史探索"] },
    { id: "feishu", label: "飞书自建应用", supported: true, protocol: "飞书事件订阅 v2 与官方长连接", docs_url: "https://open.feishu.cn/", limitations: ["仅名单内用户的私聊文本"] },
  ];
  const presets = [{ id: "preset-kindness", name: "昔涟 · 温柔日常", agent_id: null, model_profile_id: null, instructions: "语气自然，先回应对方重点。", builtin: true, created_at: now, updated_at: now }];
  const qq = {
    id: "channel-qq", name: "QQ · 昔涟", provider: "onebot11", enabled: true, app_id: null,
    api_base_url: null, self_id: "90001", allowed_senders: ["1001"], allowed_groups: ["3001"], group_trigger: "mention",
    connection_mode: "reverse_websocket", preset_id: "preset-kindness", reply_mode: "manual", status: "receiving",
    last_received_at: now, last_event: null, last_error: null, created_at: now, updated_at: now,
    callback_path: "/api/channel-callbacks/channel-qq", credentials_configured: true,
  };
  const wechat = {
    id: "channel-wechat-onebot", name: "微信 · Akasha", provider: "wechat_onebot", enabled: true, app_id: null,
    api_base_url: null, self_id: "90002", allowed_senders: [], allowed_groups: [], group_trigger: "mention",
    connection_mode: "reverse_websocket", preset_id: null, reply_mode: "manual", status: "connected",
    last_received_at: now, last_event: null, last_error: null, created_at: now, updated_at: now,
    callback_path: "/api/channel-callbacks/channel-wechat-onebot", credentials_configured: true,
  };
  const wechatIlink = {
    id: "channel-wechat-ilink", name: "微信 · iLink", provider: "wechat_ilink", enabled: true, app_id: null,
    api_base_url: null, self_id: null, allowed_senders: [], allowed_groups: [], group_trigger: "mention",
    connection_mode: "polling", preset_id: null, reply_mode: "manual", status: "awaiting_login",
    last_received_at: null, last_event: null, last_error: null, created_at: now, updated_at: now,
    callback_path: "/api/channel-callbacks/channel-wechat-ilink", credentials_configured: false,
  };
  const qqEmpty = {
    id: "channel-qq-empty", name: "QQ · 空会话测试", provider: "onebot11", enabled: true, app_id: null,
    api_base_url: null, self_id: "90004", allowed_senders: ["1001"], allowed_groups: [], group_trigger: "mention",
    connection_mode: "reverse_websocket", preset_id: "preset-kindness", reply_mode: "manual", status: "connected",
    last_received_at: null, last_event: null, last_error: null, created_at: now, updated_at: now,
    callback_path: "/api/channel-callbacks/channel-qq-empty", credentials_configured: true,
  };
  const channels = new Map([[qq.id, qq], [wechat.id, wechat], [wechatIlink.id, wechatIlink], [qqEmpty.id, qqEmpty]]);
  const conversations = [
    { id: "session-private-1", channel_id: qq.id, peer_id: "2001", peer_type: "private", conversation_id: "conversation-friend-1", created_at: now - 60000 },
    { id: "session-group-1", channel_id: qq.id, peer_id: "3001", peer_type: "group", conversation_id: "conversation-group-1", created_at: now - 120000 },
  ];
  const messages = [
    { id: "message-private-1", channel_id: qq.id, conversation_id: "conversation-friend-1", event_id: "event-private-1", external_message_id: "platform-10", sender_id: "2001", peer_type: "private", peer_id: "2001", content: "这周的活动安排有更新吗？", draft_reply: "我来把最新安排整理给你。", status: "awaiting_send", send_mode: null, error: null, reply_message_id: null, approved_at: null, created_at: now - 45000, updated_at: now - 45000 },
    { id: "message-group-1", channel_id: qq.id, conversation_id: "conversation-group-1", event_id: "event-group-1", external_message_id: "platform-11", sender_id: "2002", peer_type: "group", peer_id: "3001", content: "请帮忙看一下活动时间。", draft_reply: null, status: "queued", send_mode: null, error: null, reply_message_id: null, approved_at: null, created_at: now - 90000, updated_at: now - 90000 },
  ];
  const emptySessions = [];
  const emptyMessages = [];
  const directory = {
    friends: [{ id: "2001", name: "林小雨", peer_type: "private" }, { id: "2002", name: "阿木", peer_type: "private" }],
    groups: [{ id: "3001", name: "活动小组", peer_type: "group" }],
    recent: [{ id: "2001", name: "林小雨", peer_type: "private" }, { id: "3001", name: "活动小组", peer_type: "group" }],
  };
  let delayFriendsDirectoryOnce = true;
  let delayAliceHistoryOnce = true;
  let policy = {
    channel_id: qq.id, account_id: "", enabled: false, owner_ids: ["1001"], read_friends: true, read_groups: true,
    send_mode: "automatic", send_private_ids: ["2001"], send_group_ids: [], send_all_private: false, send_all_groups: false, updated_at: now,
  };
  let mission = null;
  let run = null;
  let automation = null;
  let login = null;
  let customPreset = null;
  const sends = [];
  const manualSendActions = [];
  const approvedSends = [];

  const fulfill = (route, body, status = 200) => route.fulfill({ status, contentType: "application/json", body: JSON.stringify(body) });
  await page.route("**/api/**", async route => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();

    if (path === "/api/health" && method === "GET") return fulfill(route, { status: "healthy", service: "controlled-fixture", version: "test", database: "healthy", policy_version: "test" });
    if (path === "/api/system/preferences" && method === "GET") return fulfill(route, {
      schema_version: 1, revision: 1, setup_completed: true,
      navigation: ["chat", "connectors", "workspaces", "capabilities", "agents", "memory", "knowledge", "system", "logs", "settings", "account"].map(id => ({ id, visible: true })),
      enabled_modules: ["gateway", "approvals", "recovery", "capabilities", "monitoring", "workflows"],
      monitor: { mode: "grid", cards: ["health", "processes", "resources", "approvals", "diagnostics"].map((id, index) => ({ id, x: index % 2 * 2, y: Math.floor(index / 2), width: 2, height: 1 })) },
    });
    if ((path === "/api/voice/session" || path === "/api/presence") && method === "GET") return fulfill(route, null);
    if (path === "/api/channels/providers" && method === "GET") return fulfill(route, { providers });
    if (path === "/api/channels" && method === "GET") return fulfill(route, { channels: [...channels.values()] });
    if (path === "/api/channels/presets" && method === "GET") return fulfill(route, { presets: customPreset ? [...presets, customPreset] : presets });
    if (path === "/api/channels/presets" && method === "POST") {
      customPreset = { id: "preset-custom", ...request.postDataJSON(), builtin: false, created_at: now, updated_at: now };
      return fulfill(route, { preset: customPreset });
    }
    if (path === "/api/channels/channel-qq/sessions" && method === "GET") return fulfill(route, { sessions: conversations });
    if (path === "/api/channels/channel-qq/messages" && method === "GET") return fulfill(route, { messages });
    if (path === "/api/channels/channel-wechat-onebot/sessions" && method === "GET") return fulfill(route, { sessions: [] });
    if (path === "/api/channels/channel-wechat-onebot/messages" && method === "GET") return fulfill(route, { messages: [] });
    if (path === "/api/channels/channel-wechat-ilink/sessions" && method === "GET") return fulfill(route, { sessions: [] });
    if (path === "/api/channels/channel-wechat-ilink/messages" && method === "GET") return fulfill(route, { messages: [] });
    if (path === "/api/channels/channel-qq-empty/sessions" && method === "GET") {
      await new Promise(resolve => setTimeout(resolve, 120));
      return fulfill(route, { sessions: emptySessions });
    }
    if (path === "/api/channels/channel-qq-empty/messages" && method === "GET") return fulfill(route, { messages: emptyMessages });
    if (path === "/api/workspaces" && method === "GET") return fulfill(route, { workspaces: [{ id: "workspace-1", name: "个人工作区", description: "", root_path: null, status: "active", created_at: now, updated_at: now }] });
    if (path === "/api/automations" && method === "GET") return fulfill(route, { automations: automation ? [automation] : [] });
    if (path === "/api/automations" && method === "POST") {
      const input = request.postDataJSON();
      automation = { id: "automation-1", revision: 1, next_run_at: now + 3600000, created_at: now, updated_at: now, ...input };
      return fulfill(route, { automation });
    }
    if (path === "/api/agents" && method === "GET") return fulfill(route, { agents: [] });
    if (path === "/api/llm/models" && method === "GET") return fulfill(route, []);
    if (path === "/api/plugins/features" && method === "GET") return fulfill(route, { features: [] });

    const channelPath = path.match(/^\/api\/channels\/([^/]+)(?:\/(.*))?$/);
    if (channelPath) {
      const channelId = decodeURIComponent(channelPath[1]);
      const rest = channelPath[2] ?? "";
      if (rest === "login" && method === "POST") {
        const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="220" height="220"><rect width="220" height="220" fill="white"/><path d="M25 25h50v50H25zm120 0h50v50h-50zM25 145h50v50H25zm88-78h18v18h-18zm0 44h18v18h-18zm31 31h31v31h-31z" fill="black"/></svg>';
        login = { status: "scanning", qr_code_data_url: "data:image/svg+xml," + encodeURIComponent(svg), expires_at: now + 300000, bot_id: null, error: null };
        return fulfill(route, { login });
      }
      if (rest === "login" && method === "GET") {
        login = { status: "need_verifycode", qr_code_data_url: null, expires_at: now + 300000, bot_id: null, error: null };
        return fulfill(route, { login });
      }
      if (rest === "login/verify" && method === "POST") {
        assert.match(request.postDataJSON().code, /^\d+$/);
        login = { status: "confirmed", qr_code_data_url: null, expires_at: null, bot_id: "90003", error: null };
        const updated = { ...channels.get(channelId), self_id: "90003", status: "connected", credentials_configured: true };
        channels.set(channelId, updated);
        return fulfill(route, { login });
      }
      if (rest === "login" && method === "DELETE") {
        login = { status: "cancelled", qr_code_data_url: null, expires_at: null, bot_id: null, error: null };
        return fulfill(route, { login });
      }
      if (rest === "robot/policy" && method === "GET") return fulfill(route, { policy });
      if (rest === "robot/policy" && method === "PUT") {
        policy = { ...request.postDataJSON(), channel_id: channelId, account_id: channels.get(channelId)?.self_id ?? "", updated_at: now };
        return fulfill(route, { policy });
      }
      if (rest === "robot/directory" && method === "GET") {
        const kind = url.searchParams.get("kind");
        if (kind === "friends" && delayFriendsDirectoryOnce) {
          delayFriendsDirectoryOnce = false;
          await new Promise(resolve => setTimeout(resolve, 220));
        }
        return fulfill(route, { items: directory[kind] ?? [] });
      }
      if (rest === "robot/history" && method === "POST") {
        const input = request.postDataJSON();
        if (input.peer_type === "private" && input.peer_id === "2001" && delayAliceHistoryOnce) {
          delayAliceHistoryOnce = false;
          await new Promise(resolve => setTimeout(resolve, 220));
        }
        const records = input.peer_type === "group"
          ? [{ message_id: 81, user_id: "2002", message: [{ type: "text", data: { text: "下周六下午进行布置。" } }], time: now / 1000 - 90 }, { message_id: 82, user_id: "1001", message: "收到，我会提前到。", time: now / 1000 - 60 }]
          : input.peer_id === "2002"
            ? [{ message_id: 73, user_id: "2002", message: "需要周日的集合地址。", time: now / 1000 - 90 }]
            : [{ message_id: 71, user_id: "2001", message: "明天下午方便吗？", time: now / 1000 - 120 }, { message_id: 72, user_id: "1001", message: [{ type: "text", data: { text: "方便，我们下午见。" } }], time: now / 1000 - 60 }];
        return fulfill(route, { data: records });
      }
      if (rest === "robot/send" && method === "POST") {
        const input = request.postDataJSON();
        sends.push(input);
        const unknown = input.text === "回执未知。";
        const action = { id: `action-${sends.length}`, run_id: null, channel_id: channelId, account_id: channels.get(channelId)?.self_id ?? "", tool_name: input.peer_type === "group" ? "send_group_msg" : "send_private_msg", peer_type: input.peer_type, peer_id: input.peer_id, arguments: input, result: unknown ? {} : { message_id: 901 + sends.length }, status: unknown ? "delivery_unknown" : "sent", platform_message_id: unknown ? null : `platform-${901 + sends.length}`, error: unknown ? "平台没有提供 message_id" : null, created_at: now + sends.length, updated_at: now + sends.length };
        manualSendActions.unshift(action);
        return fulfill(route, { action });
      }
      if (rest === "robot/missions" && method === "GET") return fulfill(route, { missions: mission ? [mission] : [] });
      if (rest === "robot/missions" && method === "POST") {
        mission = { id: "mission-1", channel_id: channelId, created_at: now, updated_at: now, ...request.postDataJSON() };
        return fulfill(route, { mission });
      }
      if (rest === "robot/missions/mission-1" && method === "PUT") {
        mission = { ...mission, ...request.postDataJSON(), updated_at: now };
        return fulfill(route, { mission });
      }
      if (rest === "robot/missions/mission-1/run" && method === "POST") {
        run = { id: "run-1", channel_id: channelId, mission_id: "mission-1", conversation_id: "conversation-run-1", user_message_id: "message-run-1", account_id: qq.self_id, origin: "desktop", owner_id: "1001", goal: mission.goal, preset_id: mission.preset_id, status: "running", answer: null, error: null, created_at: now, updated_at: now };
        return fulfill(route, { run });
      }
      if (rest === "robot/runs" && method === "GET") return fulfill(route, { runs: run ? [run] : [] });
      if (rest === "robot/sends" && method === "GET") return fulfill(route, { actions: manualSendActions });
      if (rest === "robot/runs/run-1" && method === "GET") return fulfill(route, { run, actions: [{ id: "action-explore", run_id: "run-1", channel_id: channelId, tool_name: "get_recent_contact", peer_type: "recent", peer_id: null, arguments: {}, result: { count: 2 }, status: "succeeded", platform_message_id: null, error: null, created_at: now, updated_at: now }] });
      if (rest === "robot/runs/run-1/stop" && method === "POST") {
        run = { ...run, status: "stopped", updated_at: now };
        return fulfill(route, { run });
      }
      const manualSend = rest.match(/^messages\/([^/]+)\/approve-send$/);
      if (manualSend && method === "POST") {
        const input = request.postDataJSON();
        assert.equal(input.approved, true);
        approvedSends.push(input.text);
        const index = messages.findIndex(item => item.id === decodeURIComponent(manualSend[1]));
        assert.notEqual(index, -1);
        const updated = { ...messages[index], draft_reply: input.text, status: "sent", send_mode: "manual", reply_message_id: "fixture-manual-receipt", approved_at: now, updated_at: now };
        messages.splice(index, 1, updated);
        return fulfill(route, { message: updated });
      }
      if (rest === "reply-mode" && method === "POST") return fulfill(route, { channel: { ...qq, reply_mode: request.postDataJSON().reply_mode } });
    }
    return route.continue();
  });
  await page.route("**/__fixture/channels/empty-first-message", async route => {
    emptySessions.push({ id: "session-empty-first", channel_id: qqEmpty.id, peer_id: "2004", peer_type: "private", conversation_id: "conversation-empty-first", created_at: now });
    emptyMessages.push({ id: "message-empty-first", channel_id: qqEmpty.id, conversation_id: "conversation-empty-first", event_id: "event-empty-first", external_message_id: "platform-empty-first", sender_id: "2004", peer_type: "private", peer_id: "2004", content: "新账号收到的第一条消息。", draft_reply: null, status: "queued", send_mode: null, error: null, reply_message_id: null, approved_at: null, created_at: now, updated_at: now });
    return route.fulfill({ status: 204, body: "" });
  });

  if (evidenceDir) await mkdir(evidenceDir, { recursive: true });
  const capture = async name => {
    if (evidenceDir) await page.screenshot({ path: join(evidenceDir, `${name}.png`), fullPage: false, animations: "disabled" });
  };
  const checks = [];
  const requests = [];
  page.on("request", request => {
    const url = new URL(request.url());
    if (url.pathname.startsWith("/api/channels/") || url.pathname.startsWith("/api/automations")) requests.push({ method: request.method(), path: url.pathname, body: request.postData() });
  });
  await page.addInitScript(() => {
    const originalFetch = window.fetch.bind(window);
    let failedOnce = false;
    const requestIds = [];
    window.__robotRetryRequestIds = requestIds;
    window.__channelsRobotFaults = { failNextSessionListCount: 0 };
    window.fetch = (input, init) => {
      const url = new URL(typeof input === "string" ? input : input.url, window.location.href);
      const body = typeof init?.body === "string" ? JSON.parse(init.body) : null;
      if (url.pathname.endsWith("/sessions") && window.__channelsRobotFaults.failNextSessionListCount > 0) {
        window.__channelsRobotFaults.failNextSessionListCount -= 1;
        return Promise.resolve(new Response(JSON.stringify({ error: "受控会话列表失败" }), { status: 503, headers: { "Content-Type": "application/json" } }));
      }
      if (url.pathname.endsWith("/robot/send") && body?.text === "网络恢复后重试。") {
        requestIds.push(body.request_id);
        if (!failedOnce) {
          failedOnce = true;
          return Promise.resolve(new Response(JSON.stringify({ error: "Controlled fixture HTTP failure" }), {
            status: 503,
            headers: { "Content-Type": "application/json" },
          }));
        }
      }
      return originalFetch(input, init);
    };
  });

  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(`${origin}/channels`);
  assert.equal(new URL(page.url()).pathname, "/channels");
  assert.equal(await page.title(), "忆涟千言");
  assert.equal(await page.locator(".vite-error-overlay").count(), 0);
  await page.getByRole("heading", { name: "消息工作台", exact: true }).waitFor();
  await page.getByText("QQ · 昔涟", { exact: true }).waitFor();
  await page.locator(".channel-session-list").getByRole("button", { name: /2001/ }).waitFor();
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await capture("channels-robot-conversation");
  checks.push("selected_account_conversations_show_fixture_messages_without_fake_unread_counts");

  await page.evaluate(() => { window.__channelsRobotFaults.failNextSessionListCount = 10; });
  await page.getByRole("button", { name: "刷新消息工作台", exact: true }).click();
  await page.getByRole("alert").filter({ hasText: "受控会话列表失败" }).waitFor();
  await page.evaluate(() => { window.__channelsRobotFaults.failNextSessionListCount = 0; });
  await page.getByRole("button", { name: "刷新消息工作台", exact: true }).click();
  await page.locator(".channel-session-list").getByRole("button", { name: /2001/ }).waitFor();
  assert.equal(await page.getByRole("alert").filter({ hasText: "会话列表无法加载" }).count(), 0);
  checks.push("topbar_refresh_recovers_the_session_rail_after_a_list_error");

  conversations.push({ id: "session-private-2", channel_id: qq.id, peer_id: "2003", peer_type: "private", conversation_id: "conversation-friend-3", created_at: now - 30000 });
  messages.push({ id: "message-private-2", channel_id: qq.id, conversation_id: "conversation-friend-3", event_id: "event-private-2", external_message_id: "platform-12", sender_id: "2003", peer_type: "private", peer_id: "2003", content: "下周的安排还在确认吗？", draft_reply: null, status: "queued", send_mode: null, error: null, reply_message_id: null, approved_at: null, created_at: now - 20000, updated_at: now - 20000 });
  await page.getByRole("button", { name: "刷新消息", exact: true }).click();
  await page.locator(".channel-session-list").getByRole("button", { name: /2003/ }).waitFor();
  checks.push("new_inbound_peer_appears_in_the_rail_after_the_existing_message_poll");

  const unsavedReply = "这份回复还没有发送，切换后仍要保留。";
  await page.getByRole("textbox", { name: "回复正文", exact: true }).fill(unsavedReply);
  await page.locator(".channel-session-list").getByRole("button", { name: /3001/ }).click();
  await page.locator(".channel-thread-feed").getByText("请帮忙看一下活动时间。", { exact: true }).waitFor();
  await page.locator(".channel-session-list").getByRole("button", { name: /2001/ }).click();
  assert.equal(await page.getByRole("textbox", { name: "回复正文", exact: true }).inputValue(), unsavedReply);
  await page.getByRole("button", { name: "通讯录", exact: true }).click();
  await page.getByRole("button", { name: "会话", exact: true }).click();
  assert.equal(await page.getByRole("textbox", { name: "回复正文", exact: true }).inputValue(), unsavedReply);
  await page.locator(".channel-account-list").getByRole("button", { name: /微信 · Akasha/ }).click();
  await page.locator(".channel-account-list").getByRole("button", { name: /QQ · 昔涟/ }).click();
  assert.equal(await page.getByRole("textbox", { name: "回复正文", exact: true }).inputValue(), unsavedReply);
  checks.push("unsent_reply_draft_survives_message_account_and_section_switches");

  await page.locator(".channel-account-list").getByRole("button", { name: /空会话测试/ }).click();
  const emptyAccountListBox = await page.locator(".channel-account-list").boundingBox();
  const emptyAccountBox = await page.locator(".channel-account-list button[aria-pressed='true']").boundingBox();
  assert.ok(emptyAccountListBox && emptyAccountBox && emptyAccountBox.y + emptyAccountBox.height <= emptyAccountListBox.y + emptyAccountListBox.height + 1);
  await page.getByRole("status").filter({ hasText: "正在读取会话" }).waitFor();
  await capture("channels-robot-sessions-loading");
  await page.getByRole("heading", { name: "等待第一条消息", exact: true }).waitFor();
  assert.equal(await page.locator(".channel-session-list button").count(), 0);
  await capture("channels-robot-empty-sessions");
  await page.evaluate(() => fetch("/__fixture/channels/empty-first-message"));
  await page.locator(".channel-session-list").getByRole("button", { name: /2004/ }).waitFor();
  await page.locator(".channel-thread-feed").getByText("新账号收到的第一条消息。", { exact: true }).waitFor();
  await capture("channels-robot-first-inbound");
  checks.push("first_inbound_message_creates_a_session_from_the_existing_inbox_poll");
  await page.locator(".channel-account-list").getByRole("button", { name: /QQ · 昔涟/ }).click();
  await page.getByRole("textbox", { name: "回复正文", exact: true }).waitFor();
  assert.equal(await page.getByRole("textbox", { name: "回复正文", exact: true }).inputValue(), unsavedReply);

  await page.getByRole("button", { name: "审阅并批准发送", exact: true }).click();
  const manualReview = page.getByRole("dialog", { name: "确认发送外部消息", exact: true });
  await manualReview.waitFor();
  await manualReview.getByText("2001", { exact: false }).waitFor();
  await manualReview.getByRole("button", { name: "批准并发送此消息", exact: true }).click();
  await page.getByText("发送确认标识：fixture-manual-receipt", { exact: true }).waitFor();
  assert.equal(approvedSends.length, 1);
  checks.push("existing_inbound_reply_still_requires_manual_review_and_platform_receipt");

  const staleFriendsResponse = page.waitForResponse(response => {
    const url = new URL(response.url());
    return url.pathname.endsWith("/robot/directory") && url.searchParams.get("kind") === "friends";
  });
  const staleFriendsRequest = page.waitForRequest(request => {
    const url = new URL(request.url());
    return url.pathname.endsWith("/robot/directory") && url.searchParams.get("kind") === "friends";
  });
  await page.getByRole("button", { name: "通讯录", exact: true }).click();
  await staleFriendsRequest;
  await page.getByRole("button", { name: "群", exact: true }).click();
  await page.getByRole("button", { name: /活动小组.*3001/ }).waitFor();
  await staleFriendsResponse;
  assert.equal(await page.locator(".channel-directory-items").getByRole("button", { name: /2002/ }).count(), 0);
  await page.getByRole("button", { name: "好友", exact: true }).click();
  await page.getByRole("button", { name: /林小雨.*2001/ }).waitFor();
  const aliceHistoryResponse = page.waitForResponse(response => {
    if (!new URL(response.url()).pathname.endsWith("/robot/history")) return false;
    return response.request().postDataJSON().peer_id === "2001";
  });
  const aliceHistoryRequest = page.waitForRequest(request => request.url().includes("/robot/history") && request.postDataJSON().peer_id === "2001");
  await page.getByRole("button", { name: /林小雨.*2001/ }).click();
  await page.getByRole("button", { name: "读取最近 20 条", exact: true }).click();
  await aliceHistoryRequest;
  await page.getByRole("button", { name: /阿木.*2002/ }).click();
  await page.getByRole("button", { name: "读取最近 20 条", exact: true }).click();
  await page.getByText("需要周日的集合地址。", { exact: true }).waitFor();
  await aliceHistoryResponse;
  assert.equal(await page.getByText("明天下午方便吗？", { exact: true }).count(), 0);
  checks.push("late_directory_and_history_responses_do_not_replace_the_current_peer");
  await page.getByRole("button", { name: /林小雨.*2001/ }).click();
  await page.getByRole("button", { name: "读取最近 20 条", exact: true }).click();
  await page.getByText("明天下午方便吗？", { exact: true }).waitFor();
  assert.ok(requests.some(request => request.method === "POST" && request.path.endsWith("/robot/history")));
  await page.getByRole("button", { name: "群", exact: true }).click();
  await page.getByRole("button", { name: /活动小组.*3001/ }).click();
  await page.getByRole("button", { name: "读取最近 20 条", exact: true }).click();
  await page.getByText("下周六下午进行布置。", { exact: true }).waitFor();
  await page.getByRole("button", { name: "好友", exact: true }).click();
  await page.getByRole("button", { name: /林小雨.*2001/ }).click();
  await page.getByRole("button", { name: "读取最近 20 条", exact: true }).click();
  await page.getByText("明天下午方便吗？", { exact: true }).waitFor();
  await page.getByRole("textbox", { name: "主动发送内容", exact: true }).fill("我明天下午有空。\n我们当面确认安排。 ");
  await page.getByRole("button", { name: "审阅消息", exact: true }).click();
  const confirmation = page.getByRole("dialog", { name: "确认发送消息", exact: true });
  await confirmation.waitFor();
  await confirmation.getByText(/林小雨/).waitFor();
  await confirmation.getByText("发信账号：90001", { exact: true }).waitFor();
  await confirmation.getByText("我们当面确认安排。", { exact: false }).waitFor();
  await capture("channels-robot-send-review");
  await confirmation.getByRole("button", { name: "确认并发送", exact: true }).click();
  await page.getByText(/平台回执：platform-902/).waitFor();
  assert.equal(sends.length, 1);
  assert.equal(sends[0].approved, true);
  assert.match(sends[0].request_id, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  const { request_id: privateRequestId, ...privateSend } = sends[0];
  assert.deepEqual(privateSend, { account_id: "90001", peer_type: "private", peer_id: "2001", text: "我明天下午有空。\n我们当面确认安排。", approved: true });
  checks.push("history_source_is_real_endpoint_and_proactive_send_requires_destination_and_body_review");

  await page.getByRole("textbox", { name: "主动发送内容", exact: true }).fill("网络恢复后重试。");
  await page.getByRole("button", { name: "审阅消息", exact: true }).click();
  const retryConfirmation = page.getByRole("dialog", { name: "确认发送消息", exact: true });
  await retryConfirmation.getByRole("button", { name: "确认并发送", exact: true }).click();
  await retryConfirmation.getByRole("alert").waitFor();
  const originalRetryRequestId = await page.evaluate(() => window.__robotRetryRequestIds[0]);
  await retryConfirmation.getByRole("button", { name: "返回编辑", exact: true }).click();
  assert.equal(await page.getByRole("textbox", { name: "主动发送内容", exact: true }).inputValue(), "网络恢复后重试。");
  await page.getByRole("button", { name: "审阅消息", exact: true }).click();
  const reopenedRetryConfirmation = page.getByRole("dialog", { name: "确认发送消息", exact: true });
  await reopenedRetryConfirmation.waitFor();
  const retryResponse = page.waitForResponse(response => response.url().endsWith("/robot/send") && response.status() === 200);
  await reopenedRetryConfirmation.getByRole("button", { name: "确认并发送", exact: true }).click();
  await retryResponse;
  const retryRequestIds = await page.evaluate(() => window.__robotRetryRequestIds);
  assert.equal(retryRequestIds.length, 2);
  assert.equal(retryRequestIds[1], retryRequestIds[0]);
  assert.equal(retryRequestIds[0], originalRetryRequestId);
  const failedRequestId = retryRequestIds[0];
  assert.equal(sends[1].request_id, failedRequestId);
  await page.getByText(/平台回执：platform-903/).waitFor();
  checks.push("transport_error_retry_reuses_the_same_request_id_for_the_open_send_confirmation");

  await page.getByRole("button", { name: "群", exact: true }).click();
  await page.getByRole("button", { name: /活动小组.*3001/ }).click();
  await page.getByRole("textbox", { name: "主动发送内容", exact: true }).fill("我会按时到场。");
  await page.getByRole("button", { name: "审阅消息", exact: true }).click();
  const groupConfirmation = page.getByRole("dialog", { name: "确认发送消息", exact: true });
  await groupConfirmation.waitFor();
  await groupConfirmation.getByText(/活动小组/).waitFor();
  await groupConfirmation.getByRole("button", { name: "确认并发送", exact: true }).click();
  await page.getByText(/平台回执：platform-904/).waitFor();
  assert.equal(sends.length, 3);
  assert.match(sends[2].request_id, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  assert.notEqual(sends[2].request_id, privateRequestId);
  assert.notEqual(sends[2].request_id, failedRequestId);
  const { request_id: groupRequestId, ...groupSend } = sends[2];
  assert.notEqual(groupRequestId, privateRequestId);
  assert.deepEqual(groupSend, { account_id: "90001", peer_type: "group", peer_id: "3001", text: "我会按时到场。", approved: true });
  checks.push("proactive_group_send_requires_the_same_explicit_destination_review");

  await page.getByRole("button", { name: "好友", exact: true }).click();
  await page.getByRole("button", { name: /林小雨.*2001/ }).click();
  await page.getByRole("textbox", { name: "主动发送内容", exact: true }).fill("回执未知。");
  await page.getByRole("button", { name: "审阅消息", exact: true }).click();
  const unknownConfirmation = page.getByRole("dialog", { name: "确认发送消息", exact: true });
  await unknownConfirmation.getByRole("button", { name: "确认并发送", exact: true }).click();
  await page.getByText(/送达状态未知/).waitFor();
  await page.getByRole("textbox", { name: "主动发送内容", exact: true }).fill("回执未知。");
  assert.equal(await page.getByRole("button", { name: "审阅消息", exact: true }).isDisabled(), true);
  assert.equal(sends.length, 4);
  checks.push("delivery_unknown_intent_has_no_direct_retry_action");

  await page.getByRole("button", { name: "设置", exact: true }).click();
  const settings = page.getByRole("dialog", { name: "QQ 机器人设置", exact: true });
  await settings.waitFor();
  await settings.getByRole("checkbox", { name: "启用机器人自主模式" }).check();
  await settings.getByRole("textbox", { name: "主人 QQ 号", exact: true }).fill("1001\n1002");
  await settings.getByRole("checkbox", { name: "允许读取好友" }).check();
  await settings.getByRole("checkbox", { name: "允许读取群" }).check();
  await settings.getByRole("checkbox", { name: "向全部好友自动发送" }).uncheck();
  await settings.getByRole("textbox", { name: "允许自动发送的好友 ID" }).fill("2001");
  await settings.getByRole("textbox", { name: "允许自动发送的群 ID" }).fill("");
  await capture("channels-robot-qq-policy");
  await settings.getByRole("button", { name: "保存机器人权限", exact: true }).click();
  assert.equal(policy.enabled, true);
  assert.equal(policy.account_id, "90001");
  const policyUpdate = requests.find(request => request.method === "PUT" && request.path.endsWith("/robot/policy"));
  assert.ok(policyUpdate);
  assert.equal(JSON.parse(policyUpdate.body).account_id, undefined);
  assert.deepEqual(policy.owner_ids, ["1001", "1002"]);
  assert.equal(policy.send_all_private, false);
  assert.deepEqual(policy.send_private_ids, ["2001"]);
  assert.deepEqual(policy.send_group_ids, []);
  checks.push("robot_policy_persists_owner_identity_and_separate_read_and_send_scopes");
  await settings.getByRole("button", { name: "关闭QQ 机器人设置", exact: true }).click();

  await page.getByRole("button", { name: "机器人活动", exact: true }).click();
  const manualSendHistory = page.getByLabel("手动发送记录");
  await manualSendHistory.waitFor();
  await manualSendHistory.getByText(/账号 90001/).first().waitFor();
  await manualSendHistory.getByText("平台回执：platform-904", { exact: true }).waitFor();
  checks.push("manual_send_actions_are_visible_with_the_bound_account_and_actual_platform_receipt");
  await page.getByRole("textbox", { name: "目标任务", exact: true }).fill("看看最近会话，并总结活动小组最近的安排。");
  await page.getByRole("button", { name: "保存目标", exact: true }).click();
  await page.getByText("目标已保存", { exact: true }).waitFor();
  const missionDraft = "用户尚未保存的目标补充，离开后仍应保留。";
  await page.getByRole("textbox", { name: "目标任务", exact: true }).fill(missionDraft);
  await page.getByRole("button", { name: "新建目标", exact: true }).click();
  assert.equal(await page.getByRole("textbox", { name: "目标任务", exact: true }).inputValue(), "");
  const newMissionDraft = "另一份目标草稿。";
  await page.getByRole("textbox", { name: "目标任务", exact: true }).fill(newMissionDraft);
  await page.locator(".channel-mission-list").getByRole("button", { name: /QQ 目标任务/ }).click();
  assert.equal(await page.getByRole("textbox", { name: "目标任务", exact: true }).inputValue(), missionDraft);
  await page.getByRole("button", { name: "新建目标", exact: true }).click();
  assert.equal(await page.getByRole("textbox", { name: "目标任务", exact: true }).inputValue(), newMissionDraft);
  await page.locator(".channel-mission-list").getByRole("button", { name: /QQ 目标任务/ }).click();
  await page.getByRole("button", { name: "会话", exact: true }).click();
  await page.getByRole("button", { name: "机器人活动", exact: true }).click();
  assert.equal(await page.getByRole("textbox", { name: "目标任务", exact: true }).inputValue(), missionDraft);
  checks.push("unsaved_robot_goal_drafts_survive_new_goal_and_section_navigation");
  await page.getByRole("button", { name: "保存目标", exact: true }).click();
  await page.getByText("目标已保存", { exact: true }).waitFor();
  await page.getByRole("button", { name: "开始运行", exact: true }).click();
  await page.getByText("get_recent_contact", { exact: true }).waitFor();
  const robotLayoutBox = await page.locator(".channel-robot-layout").boundingBox();
  const scheduledBox = await page.locator(".channel-scheduled-missions").boundingBox();
  assert.ok(robotLayoutBox && scheduledBox && scheduledBox.y >= robotLayoutBox.y + robotLayoutBox.height - 1, "scheduled tasks follow the complete run detail without overlap");
  await page.getByRole("button", { name: "停止运行", exact: true }).click();
  await page.getByText("已停止", { exact: true }).waitFor();
  assert.equal(run.status, "stopped");
  checks.push("saved_goal_runs_shows_real_action_and_stops_the_active_run");

  await page.getByRole("button", { name: "计划任务", exact: true }).click();
  await page.getByRole("textbox", { name: "计划名称", exact: true }).fill("每周活动安排");
  await page.getByRole("combobox", { name: "执行频率", exact: true }).selectOption("weekly");
  await page.getByRole("checkbox", { name: "周一" }).check();
  await page.getByLabel("执行时间", { exact: true }).fill("09:30");
  await page.locator(".channel-schedule-form").getByRole("combobox", { name: "工作空间" }).selectOption("workspace-1");
  await page.getByRole("button", { name: "保存计划任务", exact: true }).click();
  assert.equal(automation.workflow_graph_id, "");
  assert.deepEqual(automation.target, { kind: "agent_turn", mission_id: "mission-1" });
  assert.equal(automation.workspace_id, "workspace-1");
  assert.deepEqual(automation.schedule.weekdays, [1]);
  checks.push("scheduled_robot_goal_is_saved_as_agent_turn_automation");

  await page.getByRole("button", { name: /微信 · Akasha/ }).click();
  await page.getByRole("button", { name: "设置", exact: true }).click();
  const wechatConfiguration = page.getByRole("dialog", { name: /微信 · Akasha/ });
  await wechatConfiguration.waitFor();
  await wechatConfiguration.getByText(/不提供 QQ 式通讯录与历史探索/).waitFor();
  assert.equal(await wechatConfiguration.getByRole("button", { name: "通讯录", exact: true }).count(), 0);
  checks.push("wechat_onebot_is_distinct_and_does_not_claim_qq_exploration_support");
  await wechatConfiguration.getByRole("button", { name: "编辑接入配置", exact: true }).click();
  const wechatEditor = page.getByRole("dialog", { name: "编辑消息渠道", exact: true });
  await wechatEditor.waitFor();
  const wechatMode = wechatEditor.getByRole("combobox", { name: "连接方式", exact: true });
  assert.equal(await wechatMode.inputValue(), "reverse_websocket");
  assert.equal(await wechatMode.getByRole("option").count(), 1);
  await wechatEditor.getByRole("textbox", { name: "Akasha OneBot Token", exact: true }).waitFor();
  await wechatEditor.getByText("群聊触发方式在 Akasha 中设置；此处的群 ID 列表只定义应用允许接收的群范围。", { exact: true }).waitFor();
  assert.equal(await wechatEditor.getByRole("combobox", { name: "群消息触发方式", exact: true }).count(), 0);
  assert.equal(await wechatEditor.getByRole("button", { name: /开始扫码/ }).count(), 0);
  checks.push("wechat_onebot_configuration_uses_only_reverse_websocket_and_no_ilink_qr_flow");
  await wechatEditor.getByRole("button", { name: "关闭编辑消息渠道", exact: true }).click();

  await page.getByRole("button", { name: /微信 · iLink/ }).click();
  await page.getByRole("button", { name: "设置", exact: true }).click();
  const ilinkSettings = page.getByRole("dialog", { name: /微信 · iLink 设置/ });
  await ilinkSettings.waitFor();
  await ilinkSettings.getByRole("button", { name: "开始扫码", exact: true }).click();
  const ilinkLogin = page.getByRole("dialog", { name: "微信扫码登录", exact: true });
  await ilinkLogin.waitFor();
  await ilinkLogin.getByRole("img", { name: "微信 iLink 登录二维码", exact: true }).waitFor();
  await capture("channels-robot-wechat-qr");
  await ilinkLogin.getByLabel("手机显示的数字验证码", { exact: true }).waitFor();
  await ilinkLogin.getByLabel("手机显示的数字验证码", { exact: true }).fill("52831");
  await ilinkLogin.getByRole("button", { name: "验证并继续", exact: true }).click();
  await ilinkLogin.getByText("登录状态：微信已确认登录", { exact: true }).waitFor();
  await ilinkLogin.getByRole("button", { name: "完成", exact: true }).click();
  await ilinkSettings.getByRole("button", { name: "关闭微信 · iLink 设置", exact: true }).click();
  assert.equal(channels.get("channel-wechat-ilink").self_id, "90003");
  checks.push("existing_ilink_qr_verification_flow_is_preserved_inside_the_settings_drawer");

  channels.set("channel-wechat-ilink", { ...channels.get("channel-wechat-ilink"), status: "error", last_error: "Controlled disconnected fixture" });
  await page.getByRole("button", { name: "刷新消息工作台", exact: true }).click();
  await page.locator(".channel-topbar-state").filter({ hasText: "渠道异常" }).waitFor();
  assert.equal(await page.locator(".channel-topbar-state").getAttribute("data-connected"), "false");
  checks.push("topbar_connection_indicator_uses_the_selected_account_status");

  await page.getByRole("button", { name: "预设", exact: true }).click();
  await page.getByRole("heading", { name: "回复预设", exact: true }).waitFor();
  await page.getByRole("button", { name: /昔涟 · 温柔日常/ }).click();
  await page.getByRole("button", { name: "复制为自定义", exact: true }).click();
  const presetDraftName = "跨页面保留的预设草稿";
  await page.getByRole("textbox", { name: "预设名称", exact: true }).fill(presetDraftName);
  await page.getByRole("link", { name: "更多", exact: true }).click();
  await page.getByRole("heading", { name: "更多", exact: true }).waitFor();
  await page.getByRole("button", { name: /消息渠道/ }).click();
  await page.getByRole("heading", { name: "回复预设", exact: true }).waitFor();
  await page.getByRole("textbox", { name: "预设名称", exact: true }).waitFor();
  assert.equal(await page.getByRole("textbox", { name: "预设名称", exact: true }).inputValue(), presetDraftName);
  await capture("channels-robot-preset-restored");
  await page.getByRole("button", { name: "保存预设", exact: true }).click();
  await page.getByRole("heading", { name: presetDraftName, exact: true }).waitFor();
  assert.equal(customPreset?.name, presetDraftName);
  checks.push("unsaved_preset_survives_app_navigation_until_explicit_save");

  await page.goto(`${origin}/channels?channel=channel-qq&run=run-1`);
  await page.getByRole("heading", { name: "机器人目标", exact: true }).waitFor();
  await page.getByText("get_recent_contact", { exact: true }).waitFor();
  checks.push("automation_history_query_selects_the_channel_and_requested_robot_run");

  await page.setViewportSize({ width: 900, height: 900 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  const railBox = await page.locator(".channel-left-rail").boundingBox();
  const mainBox = await page.locator(".channel-workspace-main").boundingBox();
  assert.ok(railBox && mainBox && mainBox.y >= railBox.y + railBox.height - 1, "narrow workspace stacks the account selector above details");
  await capture("channels-robot-narrow");

  await page.setViewportSize({ width: 760, height: 900 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  const narrowSettings = page.getByRole("dialog", { name: "QQ 机器人设置", exact: true });
  await narrowSettings.waitFor();
  const narrowDrawerBox = await narrowSettings.boundingBox();
  assert.ok(narrowDrawerBox && narrowDrawerBox.width >= 650 && narrowDrawerBox.width <= 676);
  await capture("channels-robot-mobile-settings");
  await narrowSettings.getByRole("button", { name: "关闭QQ 机器人设置", exact: true }).click();
  await capture("channels-robot-mobile");
  const result = { status: "passed", checks, requests, evidence: "All channel, robot, workspace, and automation requests were intercepted by controlled fixtures; no real account or external send was used." };
  if (evidenceDir) await writeFile(join(evidenceDir, "channels-robot.json"), JSON.stringify(result, null, 2));
  return result;
}
