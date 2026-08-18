import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import conversationSidebarSource from "../chat/ConversationSidebar.tsx?raw";
import executionSidebarSource from "../../features/execution/ExecutionSidebar.tsx?raw";

const indexCssSource = readFileSync(
  fileURLToPath(new URL("../../index.css", import.meta.url)),
  "utf8",
);

describe("Shell rail visual contracts", () => {
  it("keeps the conversation rail and execution rail presentation hooks", () => {
    expect(conversationSidebarSource).toContain("conversation-sidebar");
    expect(conversationSidebarSource).toContain("conversation-list");
    expect(conversationSidebarSource).not.toContain("overflow-y-auto scrollbar-thin");
    expect(executionSidebarSource).toContain("execution-sidebar");
    expect(executionSidebarSource).toContain("execution-sidebar-content");
  });

  it("keeps the left rail scrollbar overrides after the shared thin scrollbar utility", () => {
    const sharedScrollbarRule = indexCssSource.indexOf(".scrollbar-thin {");

    expect(indexCssSource).toContain(".nav-rail-list.scrollbar-thin");
    expect(indexCssSource).toContain(".conversation-list.scrollbar-thin");
    expect(indexCssSource.indexOf(".nav-rail-list.scrollbar-thin")).toBeGreaterThan(
      sharedScrollbarRule,
    );
    expect(
      indexCssSource.indexOf(".conversation-list.scrollbar-thin"),
    ).toBeGreaterThan(sharedScrollbarRule);
  });
});
