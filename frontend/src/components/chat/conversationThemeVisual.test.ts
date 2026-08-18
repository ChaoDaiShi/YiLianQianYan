import { describe, expect, it } from "vitest";
// The frontend tsconfig intentionally has no Node typings; Vitest supplies this module at test time.
// @ts-expect-error -- Node's file reader is test-only and is not bundled into the app.
import { readFileSync } from "node:fs";
import chatPageSource from "../../pages/ChatPage.tsx?raw";
import chatViewSource from "./ChatView.tsx?raw";
import messageBubbleSource from "./MessageBubble.tsx?raw";
import messageListSource from "./MessageList.tsx?raw";
import toolCallCardSource from "./ToolCallCard.tsx?raw";
import agentProgressCardSource from "./AgentProgressCard.tsx?raw";
import approvalCardSource from "../approval/ApprovalCard.tsx?raw";
import executionHistorySource from "../../features/execution/ExecutionHistory.tsx?raw";

const indexCssSource = readFileSync(
  new URL("../../index.css", import.meta.url),
  "utf8",
);

describe("R1B conversation visual contracts", () => {
  it("marks active conversation mode without changing its data flow", () => {
    expect(chatPageSource).toContain('data-chat-view={conversationId ? "conversation" : "home"}');
    expect(chatViewSource).toContain("conversation-chat");
    expect(chatViewSource).toContain("conversation-composer");
  });

  it("keeps message identity and content surfaces explicit", () => {
    expect(messageBubbleSource).toContain("conversation-message-user");
    expect(messageBubbleSource).toContain("conversation-message-assistant");
    expect(messageBubbleSource).toContain("conversation-assistant-avatar");
    expect(messageListSource).toContain("conversation-streaming-message");
    expect(messageListSource).toContain("conversation-error-notice");
  });

  it("separates agent status, utility, decision, and timeline presentation", () => {
    expect(agentProgressCardSource).toContain("agent-progress-card");
    expect(toolCallCardSource).toContain("conversation-tool-card");
    expect(toolCallCardSource).toContain("tool-card-icon-well");
    expect(toolCallCardSource).toContain("data-status");
    expect(approvalCardSource).toContain("conversation-approval-card");
    expect(executionHistorySource).toContain("execution-history-item");
  });

  it("defines the quieter conversation atmosphere and markdown hierarchy", () => {
    expect(indexCssSource).toContain('.workbench-grid[data-chat-view="conversation"]');
    expect(indexCssSource).toContain(".conversation-message-assistant");
    expect(indexCssSource).toContain(".conversation-tool-card");
    expect(indexCssSource).toContain(".conversation-composer");
    expect(indexCssSource).toContain(".conversation-markdown");
  });
});
