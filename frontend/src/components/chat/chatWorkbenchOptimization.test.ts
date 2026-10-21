import { describe, expect, it } from "vitest";
import chatPageSource from "../../pages/ChatPage.tsx?raw";
import chatViewSource from "./ChatView.tsx?raw";
import chatInputSource from "./ChatInput.tsx?raw";
import messageBubbleSource from "./MessageBubble.tsx?raw";
import conversationSidebarSource from "./ConversationSidebar.tsx?raw";
import toolCallCardSource from "./ToolCallCard.tsx?raw";
import approvalCardSource from "../approval/ApprovalCard.tsx?raw";
import executionHistorySource from "../../features/execution/ExecutionHistory.tsx?raw";
import { stylesheetText } from "../../styles/stylesheet";

describe("Work Package B: Chat Workbench Optimization Contracts", () => {
  describe("B1 Layout & Shortcuts", () => {
    it("adds .ui-opt-chat class to ChatPage root", () => {
      expect(chatPageSource).toContain("ui-opt-chat");
      expect(stylesheetText).toContain(".ui-opt-chat");
    });

    it("wires Ctrl+B and Ctrl+J shortcuts guarded against IME and input focus", () => {
      expect(chatPageSource).toContain('event.key === "b" || event.key === "B"');
      expect(chatPageSource).toContain('event.key === "j" || event.key === "J"');
      expect(chatPageSource).toContain("event.isComposing");
      expect(chatPageSource).toContain("event.keyCode === 229");
      expect(chatPageSource).toContain('target.tagName === "INPUT"');
      expect(chatPageSource).toContain('target.tagName === "TEXTAREA"');
      expect(chatPageSource).toContain("target.closest(\"[role='dialog']\")");
    });

    it("enforces central content width max 860px and alignment in CSS", () => {
      expect(stylesheetText).toContain("width: min(100%, 860px);");
      expect(stylesheetText).toContain(".conversation-message-list-inner");
      expect(stylesheetText).toContain(".conversation-composer-inner");
      expect(stylesheetText).toContain("max-width: min(75%, 720px);");
      expect(stylesheetText).toContain("@media (max-width: 959px)");
    });
  });

  describe("B2 Conversation History & Reading", () => {
    it("groups conversation sidebar by 今天, 近7天, 更早", () => {
      expect(conversationSidebarSource).toContain("今天");
      expect(conversationSidebarSource).toContain("近7天");
      expect(conversationSidebarSource).toContain("更早");
      expect(conversationSidebarSource).toContain("startOfToday");
      expect(conversationSidebarSource).toContain("sevenDaysAgo");
      expect(conversationSidebarSource).toContain("conversation-row-action");
    });

    it("does not introduce unbacked pin or rename APIs", () => {
      expect(conversationSidebarSource).not.toContain("置顶");
      expect(conversationSidebarSource).not.toContain("重命名");
    });

    it("handles clipboard copy success ('已复制') and failure ('复制失败')", () => {
      expect(messageBubbleSource).toContain("已复制");
      expect(messageBubbleSource).toContain("复制失败");
      expect(messageBubbleSource).toContain("navigator.clipboard.writeText");
      expect(messageBubbleSource).toContain("copyStatus");
    });

    it("isolates code blocks and markdown tables to internal scrolling", () => {
      expect(messageBubbleSource).toContain("table-scroll-container");
      expect(messageBubbleSource).toContain("code-scroll-container");
      expect(stylesheetText).toContain(".code-scroll-container");
      expect(stylesheetText).toContain(".table-scroll-container");
    });
  });

  describe("B3 Input Dock & Send/Stop", () => {
    it("keeps compact mode-dependent textarea limits and the IME guard on Enter", () => {
      expect(chatInputSource).toContain("compact-composer");
      expect(chatInputSource).toContain('workspaceMode === "chat"');
      expect(chatInputSource).toContain("Math.min(textarea.scrollHeight, 120)");
      expect(chatInputSource).toContain("event.nativeEvent.isComposing || event.keyCode === 229");
    });

    it("uses Button loading for submitting and retains draft on error", () => {
      expect(chatInputSource).toContain('loading={sending}');
      expect(chatInputSource).toContain('loadingLabel="正在提交"');
      expect(chatInputSource).toContain('停止');
      expect(chatInputSource).toContain("onStop");
      expect(chatInputSource).toContain("conversation-composer-inner");
    });
  });

  describe("B4 Scrolling & Execution Summaries", () => {
    it("tracks isNearBottom (48px) and provides 回到最新 button", () => {
      expect(chatViewSource).toContain("isNearBottom(event.currentTarget, 48)");
      expect(chatViewSource).toContain("回到最新");
      expect(chatViewSource).toContain("scrollToLatest");
      expect(chatViewSource).toContain("chat-scroll-bottom-btn");
    });

    it("does not fabricate timer elapsed without start and finish timestamps", () => {
      expect(toolCallCardSource).toContain("startedAt !== undefined && finishedAt !== undefined");
      expect(executionHistorySource).toContain("record.startedAt !== undefined && record.finishedAt !== undefined");
    });
  });

  describe("B5 Approval Card & Verification State", () => {
    it("uses Button loading for approval actions and prevents horizontal clipping", () => {
      expect(approvalCardSource).toContain("loading={resolving}");
      expect(approvalCardSource).toContain('loadingLabel="提交中…"');
      expect(approvalCardSource).toContain("approval-card-actions");
      expect(approvalCardSource).toContain("flex-wrap");
    });

    it("explicitly presents verification failures in ToolCallCard and ExecutionHistory", () => {
      expect(toolCallCardSource).toContain('verificationStatus === "failed" ? "验证失败" : formatToolStatus(effectiveStatus)');
      expect(toolCallCardSource).toContain('statusTone(effectiveStatus, verificationStatus)');
      expect(executionHistorySource).toContain('record.verificationStatus === "failed"');
    });
  });
});
