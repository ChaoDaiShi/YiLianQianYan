import {describe,expect,it} from "vitest";
import source from "../../surfaces/workspace/WorkspaceSurface.tsx?raw";
import {NAV_GROUPS} from "./navGroups";
import { TASK_GRAPH_WORKFLOWS, taskWorkflowDestination } from "../../features/workflows/taskGraphRoutes";
describe("workbench routing", () => {
  it("preserves the primary connectors, more and chat navigation", () => {
    expect(source).toContain('path="connectors"');
    expect(source).toContain('path="more"');
    expect(source).toContain('path="chat/:id"');
    expect(source).not.toContain('path="search"');
    expect(NAV_GROUPS.flatMap(group => group.items).map(item => item.to)).toContain("/connectors");
  });

  it("keeps saved goal destinations reachable from conversations and workspaces", () => {
    expect(source).toContain('path="tasks" element={<Navigate to={taskWorkflowDestination(search)} replace />}');
    expect(source).toContain('path="task-world/:graphId" element={<TaskWorldPage />}');
    expect(taskWorkflowDestination("")).toBe(TASK_GRAPH_WORKFLOWS);
    expect(taskWorkflowDestination("?mode=goal&workspace=scope%2Fone")).toBe(`${TASK_GRAPH_WORKFLOWS}&workspace=scope%2Fone&mode=goal`);
    expect(taskWorkflowDestination("?view=outputs")).toBe("/workspaces?view=outputs");
  });
});
