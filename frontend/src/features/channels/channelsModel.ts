import type { ChannelMessage, ChannelReplyMode } from "../../api/channels";

const channelLabels: Record<string, string> = {
  disabled: "已停用",
  configured: "已配置 · 等待连接",
  connecting: "正在连接",
  connected: "已连接",
  awaiting_login: "等待微信登录",
  receiving: "已收到有效消息",
  error: "渠道异常",
};

const messageLabels: Record<string, string> = {
  queued: "等待处理",
  running: "正在生成草稿",
  awaiting_send: "草稿待审阅",
  failed: "回复生成失败",
  sending: "正在发送",
  sent: "平台已确认受理",
  delivery_unknown: "送达状态未知",
  send_failed: "发送失败",
  dismissed: "已放弃草稿",
};

const loginLabels: Record<string, string> = {
  wait: "等待扫码",
  scanning: "等待扫码",
  scanned: "已扫码，等待手机确认",
  scaned: "已扫码，等待手机确认",
  scaned_but_redirect: "已扫码，正在切换登录节点",
  binded_redirect: "账号已绑定，请重新开始登录",
  verify_code_blocked: "验证码验证受限，请重新扫码",
  not_started: "尚未开始扫码",
  need_verifycode: "等待输入手机验证码",
  confirmed: "微信已确认登录",
  connected: "微信已确认登录",
  logged_in: "微信已确认登录",
  expired: "二维码已过期",
  cancelled: "已取消",
  error: "登录失败",
};

export function channelStatusLabel(status: string) {
  return channelLabels[status] ?? status;
}

export function messageStatusLabel(status: string, sendMode: ChannelMessage["send_mode"] = null) {
  if (status === "sent" && sendMode === "automatic") return "自动回复 · 平台已确认受理";
  if (status === "sent" && sendMode === "manual") return "人工审阅 · 平台已确认受理";
  return messageLabels[status] ?? status;
}

export function replyModeLabel(mode: ChannelReplyMode) {
  return mode === "automatic" ? "自动回复" : "审阅后发送";
}

export function replyModeDescription(mode: ChannelReplyMode) {
  return mode === "automatic"
    ? "新私聊生成回复后直接发送；已有草稿保持原状。"
    : "新私聊生成草稿后等待你审阅并批准；已有草稿保持原状。";
}

export function isChannelLoginComplete(status: string) {
  return status === "confirmed" || status === "connected" || status === "logged_in";
}

export function channelLoginStatusLabel(status: string) {
  return loginLabels[status] ?? status;
}

export function canEditChannelReply(status: ChannelMessage["status"]) {
  return status === "awaiting_send" || status === "send_failed";
}

export function canSendChannelReply(status: ChannelMessage["status"], enabled: boolean) {
  return enabled && (status === "awaiting_send" || status === "send_failed");
}

export function parseAllowedSenders(value: string) {
  return [...new Set(value.split(/[\s,，]+/).map(item => item.trim()).filter(Boolean))];
}

export interface ConnectionModeOption {
  value: ChannelInputMode;
  label: string;
}

export type ChannelInputMode = "http" | "websocket" | "reverse_websocket" | "polling";

export function connectionModesForProvider(provider: string): ConnectionModeOption[] {
  if (provider === "feishu") {
    return [
      { value: "websocket", label: "飞书官方长连接" },
      { value: "http", label: "HTTP 事件回调" },
    ];
  }
  if (provider === "onebot11") {
    return [
      { value: "http", label: "HTTP 反向上报" },
      { value: "reverse_websocket", label: "反向 WebSocket" },
    ];
  }
  if (provider === "wechat_onebot") {
    return [{ value: "reverse_websocket", label: "Akasha-WeChat 反向 WebSocket" }];
  }
  if (provider === "wechat_ilink") {
    return [{ value: "polling", label: "微信 iLink 轮询" }];
  }
  return [];
}

export function connectionModeLabel(mode: string) {
  return connectionModesForProvider("feishu").concat(
    connectionModesForProvider("onebot11"),
    connectionModesForProvider("wechat_onebot"),
    connectionModesForProvider("wechat_ilink"),
  ).find(item => item.value === mode)?.label ?? mode;
}

export function channelReceiveAddress(provider: string, mode: string, callbackPath: string, apiBase: string) {
  if (!(["onebot11", "wechat_onebot"].includes(provider)) || (mode !== "http" && mode !== "reverse_websocket")) return null;
  const target = new URL(callbackPath, apiBase);
  if (mode === "reverse_websocket") {
    target.protocol = target.protocol === "https:" ? "wss:" : "ws:";
    target.pathname = `${target.pathname.replace(/\/$/, "")}/ws`;
  }
  return target.toString();
}
