import { describe, expect, it } from "vitest";
import conversationSidebarSource from "../chat/ConversationSidebar.tsx?raw";
import executionSidebarSource from "../../features/execution/ExecutionSidebar.tsx?raw";
import appShellSource from "./AppShell.tsx?raw";
import navRailSource from "./NavRail.tsx?raw";

describe("Shell rail visual contracts", () => {
  it("keeps the conversation rail and execution rail presentation hooks", () => {
    expect(conversationSidebarSource).toContain("conversation-sidebar");
    expect(conversationSidebarSource).toContain("conversation-list");
    expect(conversationSidebarSource).not.toContain("overflow-y-auto scrollbar-thin");
    expect(executionSidebarSource).toContain("execution-sidebar");
    expect(executionSidebarSource).toContain("execution-sidebar-content");
  });

  it("defines shell-wide artwork and adaptive nav rail hooks", () => {
    expect(appShellSource).toContain('className="shell-ambient');
    expect(appShellSource).toContain('className="shell-ambient-overlay');
    expect(navRailSource).toContain("nav-rail-compact");
    expect(navRailSource).toContain("nav-rail-item");
  });
});
