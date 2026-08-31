import { describe, expect, it } from "vitest";
import * as compatibility from "./client";
import * as conversations from "./conversations";
import * as tasks from "./tasks";
import * as workflows from "./workflows";
import * as workspaces from "./workspaces";
import * as capabilities from "./capabilities";

describe("frontend API domain boundary", () => {
  it("keeps legacy imports source compatible through the client facade", () => {
    expect(compatibility.listConversations).toBe(conversations.listConversations);
    expect(compatibility.listTasks).toBe(tasks.listTasks);
    expect(compatibility.listWorkflowGraphs).toBe(workflows.listWorkflowGraphs);
    expect(compatibility.listWorkspaces).toBe(workspaces.listWorkspaces);
    expect(compatibility.listCapabilities).toBe(capabilities.listCapabilities);
  });
});
