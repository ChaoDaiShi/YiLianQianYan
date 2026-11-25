export const TASK_GRAPH_WORKFLOWS = "/capabilities?category=workflow&workflow_tab=taskgraphs";

export function taskGraphPath(id: string) {
  return `/task-world/${encodeURIComponent(id)}`;
}

export function taskWorkflowDestination(search: string) {
  const incoming = new URLSearchParams(search);
  if (incoming.get("view") === "outputs") return "/workspaces?view=outputs";
  const scope = new URLSearchParams();
  const workspace = incoming.get("workspace");
  if (workspace) scope.set("workspace", workspace);
  if (incoming.get("mode") === "goal") scope.set("mode", "goal");
  return scope.size ? `${TASK_GRAPH_WORKFLOWS}&${scope}` : TASK_GRAPH_WORKFLOWS;
}
