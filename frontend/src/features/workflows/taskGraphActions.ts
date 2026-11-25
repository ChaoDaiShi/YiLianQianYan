import { cancelTaskCommand, cancelTaskExecution, createTaskGraph, deleteTaskGraph, getTaskGraphDetail, type ApiError, type TaskGraphDetail } from "../../api/taskWorld";
import { taskGraphPath } from "./taskGraphRoutes";

interface EmptyTaskGraphCreationDependencies {
  createGraph: typeof createTaskGraph;
  navigate: (path: string) => void;
  setCreating: (creating: boolean) => void;
  setError: (error: ApiError | null) => void;
  inFlight: { current: boolean };
  makeGraphId: () => string;
}

export async function createEmptyTaskGraph({ createGraph, navigate, setCreating, setError, inFlight, makeGraphId }: EmptyTaskGraphCreationDependencies) {
  if (inFlight.current) return;
  inFlight.current = true;
  setCreating(true);
  setError(null);
  try {
    const result = await createGraph({ id: makeGraphId(), nodes: [], edges: [] });
    if (!result.ok) { setError(result.error); return; }
    if (typeof result.data?.id !== "string" || !result.data.id.trim()) throw new Error("创建响应缺少画布标识，请刷新工作流后重试。");
    navigate(taskGraphPath(result.data.id));
  } catch (error) {
    setError({ status: 0, code: "creation_failed", message: error instanceof Error ? error.message : String(error) });
  } finally {
    inFlight.current = false;
    setCreating(false);
  }
}

const activeStatuses = new Set(["pending", "ready", "dispatching", "waiting_approval", "running", "validating"]);
export function activeGraphExecutions(detail: TaskGraphDetail) {
 const result = new Map<string, string>();
 for (const node of detail.nodes) for (const execution of [node.latest_execution, ...(node.execution_history ?? [])]) if (execution && activeStatuses.has(execution.status)) result.set(execution.execution_id, node.id);
 return result;
}
export async function stopAndDeleteHistoricalGraph(id: string) {
 let response = await getTaskGraphDetail(id); if (!response.ok) throw new Error(response.error.message);
 for (const node of response.data.nodes) {
   const command = node.state.command_execution;
   if (command && ["dispatching", "waiting_approval", "running"].includes(command.status)) {
     const current = await getTaskGraphDetail(id); if (!current.ok) throw new Error(current.error.message);
     const stopped = await cancelTaskCommand(id, node.id, current.data.revision, command.request_id); if (!stopped.ok) throw new Error(stopped.error.message);
   }
 }
 for (const executionId of activeGraphExecutions(response.data).keys()) {
   const current = await getTaskGraphDetail(id); if (!current.ok) throw new Error(current.error.message);
   if (!activeGraphExecutions(current.data).has(executionId)) continue;
   const stopped = await cancelTaskExecution(id, executionId, current.data.revision); if (!stopped.ok) throw new Error(stopped.error.message);
 }
 const deadline = Date.now() + 15000;
 while (true) {
   response = await getTaskGraphDetail(id); if (!response.ok) throw new Error(response.error.message);
   const commands = response.data.nodes.some(node => node.state.command_execution && ["dispatching", "waiting_approval", "running"].includes(node.state.command_execution.status));
   if (!activeGraphExecutions(response.data).size && !commands) break;
   if (Date.now() >= deadline) throw new Error("执行仍在停止中，历史图已保留。请稍后重试删除。");
   await new Promise(resolve => setTimeout(resolve, 300));
 }
 const deleted = await deleteTaskGraph(id, response.data.revision); if (!deleted.ok) throw new Error(deleted.error.message);
}
