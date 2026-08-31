import { describe, expect, it } from "vitest";
import workspaceSurfaceSource from "../../surfaces/workspace/WorkspaceSurface.tsx?raw";
import navGroupsSource from "./navGroups.ts?raw";

describe("task center routing", () => {
  it("adds a task center route while preserving conversation navigation", () => {
    expect(workspaceSurfaceSource).toContain('path="tasks"');
    expect(workspaceSurfaceSource).toContain("TaskCenterPage");
    expect(navGroupsSource).toContain('to: "/tasks"');
    expect(navGroupsSource).toContain('to: "/chat"');
  });
});
