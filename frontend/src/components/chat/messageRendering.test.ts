import { describe, expect, it } from "vitest";
import { areMessageBubblePropsEqual } from "./MessageBubble";
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
        { message, showAssistantAvatar: true },
        { message, showAssistantAvatar: true },
      ),
    ).toBe(true);
  });

  it("rerenders when message data or avatar visibility changes", () => {
    expect(
      areMessageBubblePropsEqual(
        { message, showAssistantAvatar: true },
        { message: { ...message, content: "更新后的内容" }, showAssistantAvatar: true },
      ),
    ).toBe(false);
    expect(
      areMessageBubblePropsEqual(
        { message, showAssistantAvatar: true },
        { message, showAssistantAvatar: false },
      ),
    ).toBe(false);
  });
});
