interface TaskGraphLookupResult {
  ok: boolean;
  data?: { graph_id?: string };
}

export async function resolveCurrentTaskDestination(
  activeTaskId: string | null,
  loadGraph: (graphId: string) => Promise<TaskGraphLookupResult>,
): Promise<string> {
  if (!activeTaskId) return "/tasks";
  try {
    const result = await loadGraph(activeTaskId);
    if (result.ok && result.data?.graph_id === activeTaskId) {
      return `/task-world/${encodeURIComponent(activeTaskId)}`;
    }
  } catch {
    // The task center owns the honest missing/error/creation state.
  }
  return "/tasks";
}
