import { describe, expect, it } from "vitest";
import { areMessageBubblePropsEqual } from "./MessageBubble";
import messageBubbleSource from "./MessageBubble.tsx?raw";
import messageListSource from "./MessageList.tsx?raw";
import type { Message } from "../../types";

const message: Message = {
  id: "assistant-1",
  role: "assistant",
  content: "已完成。",
  created_at: 1,
};

describe("MessageBubble render boundary", () => {
  it("skips unchanged message props during streaming updates", () => {
    expect(
      areMessageBubblePropsEqual(
        { message },
        { message },
      ),
    ).toBe(true);
  });

  it("rerenders when message data changes", () => {
    expect(
      areMessageBubblePropsEqual(
        { message },
        { message: { ...message, content: "更新后的内容" } },
      ),
    ).toBe(false);
  });

  it("keeps assistant identity quiet and groups consecutive messages", () => {
    expect(messageBubbleSource).toContain("showAssistantAvatar");
    expect(messageBubbleSource).toContain("conversation-assistant-avatar");
    expect(messageBubbleSource).not.toContain("/favicon.png");
    expect(messageBubbleSource).not.toContain("ml-11");
    expect(messageListSource).toContain("showAssistantAvatar");
    expect(messageListSource).not.toContain("/favicon.png");
    expect(messageListSource).not.toContain("ml-11");
  });

  it("rerenders when the assistant avatar group boundary changes", () => {
    expect(
      areMessageBubblePropsEqual(
        { message, showAssistantAvatar: true },
        { message, showAssistantAvatar: false },
      ),
    ).toBe(false);
  });

  it("routes markdown web links through the external browser opener", () => {
    expect(messageBubbleSource).toContain("openExternalUrl");
    expect(messageBubbleSource).toContain("event.preventDefault()");
  });

  it("shows a readable error message and keeps raw fields in an error log", () => {
    expect(messageListSource).toContain("presentExecutionError(error)");
    expect(messageListSource).toContain("presentation.message");
    expect(messageListSource).toContain("查看错误日志");
    expect(messageListSource).toContain("presentation.technical");
  });
});
