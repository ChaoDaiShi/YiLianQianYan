import { describe, expect, it } from "vitest";
import { canEditChannelReply, canSendChannelReply, channelLoginStatusLabel, channelReceiveAddress, channelStatusLabel, isChannelLoginComplete, messageStatusLabel, parseAllowedSenders, replyModeDescription, replyModeLabel } from "./channelsModel";

describe("channel inbox decisions", () => {
  it("requires an enabled channel and a reviewable state before sending", () => {
    expect(canEditChannelReply("awaiting_send")).toBe(true);
    expect(canEditChannelReply("send_failed")).toBe(true);
    expect(canEditChannelReply("failed")).toBe(false);
    expect(canSendChannelReply("awaiting_send", true)).toBe(true);
    expect(canSendChannelReply("send_failed", true)).toBe(true);
    expect(canSendChannelReply("awaiting_send", false)).toBe(false);
    expect(canSendChannelReply("delivery_unknown", true)).toBe(false);
    expect(canSendChannelReply("sent", true)).toBe(false);
  });

  it("keeps uncertain delivery visible and preserves unknown server states", () => {
    expect(messageStatusLabel("delivery_unknown")).toContain("未知");
    expect(messageStatusLabel("platform_waiting_new_state")).toBe("platform_waiting_new_state");
    expect(channelStatusLabel("awaiting_login")).toContain("登录");
  });

  it("parses the explicit allow-list without retaining blanks or duplicates", () => {
    expect(parseAllowedSenders("user-a, user-b\nuser-a，user-c")).toEqual(["user-a", "user-b", "user-c"]);
  });

  it("treats the iLink confirmed state as login completion without implying a received message", () => {
    expect(isChannelLoginComplete("confirmed")).toBe(true);
    expect(isChannelLoginComplete("connected")).toBe(true);
    expect(isChannelLoginComplete("awaiting_confirmation")).toBe(false);
  });

  it("shows QR verification status in plain language and keeps future states visible", () => {
    expect(channelLoginStatusLabel("need_verifycode")).toContain("验证码");
    expect(channelLoginStatusLabel("scaned")).toContain("等待手机确认");
    expect(channelLoginStatusLabel("confirmed")).toContain("登录");
    expect(channelLoginStatusLabel("new_platform_state")).toBe("new_platform_state");
  });

  it("builds same-machine OneBot endpoints and refuses to invent a Feishu public origin", () => {
    expect(channelReceiveAddress("onebot11", "http", "/api/channel-callbacks/bot", "http://127.0.0.1:9420"))
      .toBe("http://127.0.0.1:9420/api/channel-callbacks/bot");
    expect(channelReceiveAddress("onebot11", "reverse_websocket", "/api/channel-callbacks/bot", "http://127.0.0.1:9420"))
      .toBe("ws://127.0.0.1:9420/api/channel-callbacks/bot/ws");
    expect(channelReceiveAddress("feishu", "http", "/api/channel-callbacks/bot", "http://127.0.0.1:9420")).toBeNull();
  });

  it("distinguishes automatic delivery provenance from human approval", () => {
    expect(replyModeLabel("automatic")).toContain("自动");
    expect(replyModeLabel("manual")).toContain("审阅");
    expect(replyModeDescription("automatic")).toContain("已有草稿保持原状");
    expect(messageStatusLabel("sent", "automatic")).toContain("自动回复");
    expect(messageStatusLabel("sent", "manual")).toContain("人工审阅");
    expect(messageStatusLabel("sent", null)).toBe("平台已确认受理");
  });
});
