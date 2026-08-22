import { describe, expect, it } from "vitest";
import taskCenterSource from "./TaskCenterPage.tsx?raw";

describe("task center page contract", () => {
  it("uses the existing task APIs and keeps task-to-conversation links honest", () => {
    expect(taskCenterSource).toContain("listTasks");
    expect(taskCenterSource).toContain("TaskDetailPanel");
    expect(taskCenterSource).toContain("navigate(\"/chat\")");
    expect(taskCenterSource).not.toContain("conversation_id");
  });
});
