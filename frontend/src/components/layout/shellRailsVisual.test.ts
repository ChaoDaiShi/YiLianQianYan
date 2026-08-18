import { describe, expect, it } from "vitest";
import conversationSidebarSource from "../chat/ConversationSidebar.tsx?raw";
import executionSidebarSource from "../../features/execution/ExecutionSidebar.tsx?raw";

describe("Shell rail visual contracts", () => {
  it("keeps the conversation rail and execution rail presentation hooks", () => {
    expect(conversationSidebarSource).toContain("conversation-sidebar");
    expect(conversationSidebarSource).toContain("conversation-list");
    expect(executionSidebarSource).toContain("execution-sidebar");
    expect(executionSidebarSource).toContain("execution-sidebar-content");
  });
});
