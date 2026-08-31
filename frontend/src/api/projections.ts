import { controlSessionHeaders } from "./controlSession";
import { API_BASE } from "./transport";

export interface SimulationMetadata {
  simulated: boolean;
  provider: string;
  reason?: string;
}

export interface TaskProjection {
  id: string;
  title: string;
  status: string;
  progress?: number;
  current_activity?: string;
  attention_required: boolean;
  updated_at: number;
  simulation: SimulationMetadata;
  schema_version: number;
  [key: string]: unknown;
}

export interface DesktopContextProjection {
  space_id?: string;
  focused_app?: string;
  focused_window?: string;
  available_capabilities: string[];
  updated_at: number;
  simulation: SimulationMetadata;
  schema_version: number;
  [key: string]: unknown;
}

function scopeQuery(scope?: string): string {
  return scope ? `?${new URLSearchParams({ scope })}` : "";
}

export async function listTaskProjections(scope?: string): Promise<TaskProjection[]> {
  const response = await fetch(`${API_BASE}/api/projections/tasks${scopeQuery(scope)}`, {
    headers: controlSessionHeaders(),
  });
  if (!response.ok) return [];
  const body = (await response.json()) as { tasks: TaskProjection[] };
  return body.tasks;
}

export async function getDesktopContextProjection(
  scope?: string,
): Promise<DesktopContextProjection | null> {
  const response = await fetch(
    `${API_BASE}/api/projections/desktop-context${scopeQuery(scope)}`,
    { headers: controlSessionHeaders() },
  );
  return response.ok ? (response.json() as Promise<DesktopContextProjection>) : null;
}
