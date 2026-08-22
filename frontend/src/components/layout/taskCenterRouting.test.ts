import { describe, expect, it } from "vitest";
import appSource from "../../App.tsx?raw";
import navGroupsSource from "./navGroups.ts?raw";

describe("task center routing", () => {
  it("adds a task center route while preserving conversation navigation", () => {
    expect(appSource).toContain('path="tasks"');
    expect(appSource).toContain("TaskCenterPage");
    expect(navGroupsSource).toContain('to: "/tasks"');
    expect(navGroupsSource).toContain('to: "/chat"');
  });
});
