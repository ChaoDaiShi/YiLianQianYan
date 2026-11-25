import { TASK_GRAPH_WORKFLOWS, taskGraphPath } from "../../features/workflows/taskGraphRoutes";

interface TaskGraphLookupResult { ok: boolean; data?: { graph_id?: string } }

export async function resolveCurrentTaskDestination(
  activeTaskId: string | null,
  loadGraph: (id: string) => Promise<TaskGraphLookupResult>,
): Promise<string> {
  if (!activeTaskId) return TASK_GRAPH_WORKFLOWS;
  const result = await loadGraph(activeTaskId);
  return result.ok && result.data?.graph_id === activeTaskId
    ? taskGraphPath(activeTaskId)
    : TASK_GRAPH_WORKFLOWS;
}
