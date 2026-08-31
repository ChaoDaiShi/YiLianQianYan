import { controlSessionHeaders } from "./controlSession";
import { API_BASE } from "./transport";

export type CommandStatus = "succeeded" | "failed" | "not_found";

export interface CommandError {
  code: string;
  message: string;
}

export interface CommandResult {
  request_id: string;
  status: CommandStatus;
  result?: Record<string, unknown>;
  error?: CommandError;
  schema_version: number;
  [key: string]: unknown;
}

export async function executeCommand(
  command: string,
  payload: Record<string, unknown> = {},
  requestId: string = crypto.randomUUID(),
  source = "workspace-ui",
  target?: string,
): Promise<CommandResult | null> {
  const response = await fetch(`${API_BASE}/api/commands`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...controlSessionHeaders(),
    },
    body: JSON.stringify({
      command,
      request_id: requestId,
      source,
      target,
      payload,
      schema_version: 1,
    }),
  });
  if (!response.ok) return null;
  return (await response.json()) as CommandResult;
}
