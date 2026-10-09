// Task World contract — the graph, node, execution, checkpoint and canvas
// shapes the API returns, plus the error envelope this surface uses.
//
// `ApiResult` here is NOT `core/api/http.ts`'s: the task-world endpoints
// answer with `{error, code, message}`, so this surface carries a richer
// `ApiError` than the generic `ApiFailure`. They are deliberately separate
// types with the same name; import the one whose module you are calling.
export type TaskNodeKind = "work" | "approval" | "user_checkpoint";

export type TaskNodeStatus =
  | "pending"
  | "runnable"
  | "running"
  | "succeeded"
  | "failed"
  | "blocked"
  | "cancelled"
  | "invalidated";

export type TaskExecutionStatus =
  | "pending"
  | "ready"
  | "dispatching"
  | "waiting_approval"
  | "running"
  | "validating"
  | "succeeded"
  | "failed"
  | "blocked"
  | "cancelled"
  | "stale";

export interface RetryPolicy {
  max_attempts: number;
}

export interface TaskGraphNodeDefinition {
  id: string;
  kind: TaskNodeKind;
  title: string;
  input: Record<string, unknown>;
  retry_policy: RetryPolicy;
}

export interface TaskGraphDefinition {
  schema_version: number;
  id: string;
  revision: number;
  nodes: TaskGraphNodeDefinition[];
  edges: TaskEdge[];
}

export interface TaskGraphSummary {
  id: string;
  schema_version: number;
  revision: number;
  node_count: number;
  edge_count: number;
}

export interface TaskNodeStateSummary {
  status: TaskNodeStatus;
  attempts: number;
  result_summary: string | null;
  error: string | null;
  started_at: number | null;
  finished_at: number | null;
  updated_at: number;
  command_execution?: TaskCommandExecution | null;
}

export interface CommandBinding {
  command: "desktop.app.focus" | "desktop.app.open";
  args: { app_id: string };
}

export interface TaskCommandExecution {
  request_id: string;
  command: string;
  app_id: string;
  attempt: number;
  graph_revision: number;
  status: "dispatching" | "waiting_approval" | "running" | "verified" | "failed" | "cancelled";
  approval_id: string | null;
}

export interface TaskResourceReference {
  id: string;
  name: string | null;
  uri: string | null;
}

export interface TaskValidationSummary {
  status: string;
  issues: string[];
}

export interface TaskNodeDetail {
  id: string;
  kind: TaskNodeKind;
  title: string;
  retry_policy?: RetryPolicy;
  status: TaskNodeStatus;
  state: TaskNodeStateSummary;
  executor_ref: string | null;
  command_binding?: CommandBinding | null;
  instruction_summary: string;
  acceptance_criteria: string[];
  resources: TaskResourceReference[];
  validation: TaskValidationSummary;
  result_summary: string | null;
  latest_execution?: TaskNodeExecutionSummary | null;
  execution_history?: TaskNodeExecutionSummary[];
}

export interface TaskExecutionValidationSummary {
  status: "pending" | "accepted" | "rejected";
  issues: string[];
  checked_at: number | null;
}

export interface TaskNodeExecutionSummary {
  execution_id: string;
  attempt: number;
  status: TaskExecutionStatus;
  executor_ref: string | null;
  validation: TaskExecutionValidationSummary | null;
  approval_ref: string | null;
  failure_code: string | null;
  error: string | null;
  result_summary: string | null;
  created_at: number;
  updated_at: number;
  started_at: number | null;
  finished_at: number | null;
}

export interface TaskEdge {
  from: string;
  to: string;
}

export interface TaskRevisionSummary {
  graph_id: string;
  revision: number;
  change: string;
  node_count: number;
  edge_count: number;
  created_at: number;
}

export interface TaskCheckpointSummary {
  checkpoint_id: string;
  graph_id: string;
  graph_revision: number;
  created_at: number;
}

export interface TaskGraphDetail {
  graph: TaskGraphSummary;
  graph_id: string;
  revision: number;
  nodes: TaskNodeDetail[];
  edges: TaskEdge[];
  revisions: TaskRevisionSummary[];
  checkpoints: TaskCheckpointSummary[];
}

export interface CanvasViewport {
  x: number;
  y: number;
  zoom: number;
}

export interface CanvasNodeLayout {
  node_id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface CanvasGroup {
  id: string;
  title: string;
  node_ids: string[];
  collapsed: boolean;
}

export interface CanvasView {
  schema_version: number;
  graph_id: string;
  view_revision: number;
  graph_revision_seen: number;
  viewport: CanvasViewport;
  node_layouts: CanvasNodeLayout[];
  selection: string[];
  groups: CanvasGroup[];
  updated_at: number;
}

export interface ApiError {
  status: number;
  code: string;
  message: string;
}

export type ApiResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: ApiError };

export interface TaskGraphResponse {
  graph: TaskGraphDefinition;
}

export interface TaskGraphsResponse {
  graphs: TaskGraphDefinition[];
}

export interface TaskGraphDetailResponse {
  detail: TaskGraphDetail;
}

export interface CanvasViewResponse {
  view: CanvasView;
}

export interface TaskCheckpointResponse {
  checkpoint: TaskCheckpointSummary;
}

export interface TaskExecutionResponse {
  execution: TaskNodeExecutionSummary;
}

export interface TaskExecutionsResponse {
  executions: TaskNodeExecutionSummary[];
}

export interface TaskRerunResponse {
  graph_id: string;
  node_id: string;
  affected_nodes: string[];
}

export interface TaskGraphReviewSuggestion {
  suggestion_id: string;
  node_id: string;
  title: string;
  instruction: string;
  acceptance_criteria: string[];
  reason: string;
}

export interface TaskGraphReview {
  reviewed_revision: number;
  summary: string;
  suggestions: TaskGraphReviewSuggestion[];
}

export interface TaskGraphReviewResponse {
  reviewed_revision: number;
  review: Omit<TaskGraphReview, "reviewed_revision">;
}

export interface ErrorPayload {
  error?: unknown;
  message?: unknown;
}
