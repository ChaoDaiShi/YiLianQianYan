import { describe, expect, it } from "vitest";
import { areMessageBubblePropsEqual } from "./MessageBubble";
import messageBubbleSource from "./MessageBubble.tsx?raw";
import messageListSource from "./MessageList.tsx?raw";
import type { Message } from "../../types";
import { renderToStaticMarkup } from "react-dom/server";
import MessageBubble from "./MessageBubble";
import { createElement } from "react";

const message: Message = {
  id: "assistant-1",
  role: "assistant",
  content: "已完成。",
  created_at: 1,
};

describe("MessageBubble render boundary", () => {
  it("keeps answer actions visible without hover and exposes single-message deletion", () => {
    const markup = renderToStaticMarkup(createElement(MessageBubble, { message, onDelete: () => undefined }));
    expect(markup).toContain('aria-label="回答操作"');
    expect(markup).toContain('aria-label="复制回答"');
    expect(markup).toContain('aria-label="朗读回答"');
    expect(markup).toContain('aria-label="删除这条回答"');
    expect(markup).not.toContain("group-hover:opacity-100");
    expect(markup).not.toContain("opacity-0");
  });

  it("places rollback and single-message deletion on user messages and tracks action changes", () => {
    const user = { ...message, id: "user-1", role: "user" };
    const rollback = () => undefined;
    const markup = renderToStaticMarkup(createElement(MessageBubble, { message: user, onRollback: rollback, onDelete: () => undefined }));
    expect(markup).toContain('aria-label="回退这条消息"');
    expect(markup).toContain('aria-label="删除这条消息"');
    expect(areMessageBubblePropsEqual({ message: user, onRollback: rollback }, { message: user, onRollback: () => undefined })).toBe(false);
    expect(areMessageBubblePropsEqual({ message: user }, { message: user, actionsDisabled: true })).toBe(false);
  });
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

  it("renders assistant text without an avatar or reserved avatar space", () => {
    expect(messageBubbleSource).not.toContain("conversation-assistant-avatar");
    expect(messageBubbleSource).not.toContain("cyrene-home-character.png");
    expect(messageListSource).not.toContain("showAssistantAvatar");
    expect(messageBubbleSource).toContain("MessageProcess");
    const markup = renderToStaticMarkup(createElement(MessageBubble, { message: { ...message, process: { reasoning: "actual received thinking", tools: [] } } }));
    expect(markup).toContain("actual received thinking");
    expect(markup).not.toContain("<img");
  });

  it("rerenders when retained process data changes", () => {
    expect(areMessageBubblePropsEqual({ message }, { message: { ...message, process: { reasoning: "actual thinking", tools: [] } } })).toBe(false);
  });

  it("routes markdown web links through the external browser opener", () => {
    expect(messageBubbleSource).toContain("openExternalUrl");
    expect(messageBubbleSource).toContain("event.preventDefault()");
  });

  it("keeps assistant TTS manual and outside the conversation write path", () => {
    expect(messageBubbleSource).toContain("朗读回答");
    expect(messageBubbleSource).toContain("onSpeak");
    expect(messageBubbleSource).not.toContain("synthesizeSpeech(");
    expect(messageBubbleSource).not.toContain("sendMessage(");
  });

  it("shows a readable error message and keeps raw fields in an error log", () => {
    expect(messageListSource).toContain("presentExecutionError(error)");
    expect(messageListSource).toContain("presentation.message");
    expect(messageListSource).toContain("查看错误日志");
    expect(messageListSource).toContain("presentation.technical");
  });
});
