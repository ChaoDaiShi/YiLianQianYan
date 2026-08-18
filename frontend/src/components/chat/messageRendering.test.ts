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

  it("keeps assistant messages free of avatar markup and offsets", () => {
    expect(messageBubbleSource).not.toContain("showAssistantAvatar");
    expect(messageBubbleSource).not.toContain("/favicon.png");
    expect(messageBubbleSource).not.toContain("ml-11");
    expect(messageListSource).not.toContain("/favicon.png");
    expect(messageListSource).not.toContain("showAssistantAvatar");
    expect(messageListSource).not.toContain("ml-11");
  });

  it("routes markdown web links through the external browser opener", () => {
    expect(messageBubbleSource).toContain("openExternalUrl");
    expect(messageBubbleSource).toContain("event.preventDefault()");
  });
});
