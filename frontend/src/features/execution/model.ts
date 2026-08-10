export type ExecutionStatus =
  | "queued"
  | "awaiting_approval"
  | "running"
  | "interrupted"
  | "succeeded"
  | "failed"
  | "rejected"
  | "cancelled";

export type ApprovalStatus =
  | "not_required"
  | "pending"
  | "approved"
  | "rejected"
  | "cancelled"
  | "expired";

export type VerificationStatus =
  | "not_requested"
  | "pending"
  | "passed"
  | "failed";

export type ExecutionRiskLevel =
  | "low"
  | "medium"
  | "high"
  | "critical"
  | "unknown";

export type ConnectionStatus =
  | "idle"
  | "connecting"
  | "connected"
  | "interrupted"
  | "error";

export interface ExecutionRecord {
  toolCallId: string;
  conversationId: string;
  approvalId?: string;
  name: string;
  args: Record<string, unknown>;
  riskLevel: ExecutionRiskLevel;
  reason?: string;
  executionStatus: ExecutionStatus;
  approvalStatus: ApprovalStatus;
  verificationStatus: VerificationStatus;
  verificationReason?: string;
  result?: string;
  sequence: number;
}

export interface AgentRunState {
  conversationId: string | null;
  content: string;
  records: Record<string, ExecutionRecord>;
  order: string[];
  nextSequence: number;
  connection: ConnectionStatus;
  terminalError: string | null;
  terminal: boolean;
}

export interface CompletedRunSnapshot {
  conversationId: string;
  messageId: string;
  content: string;
  records: ExecutionRecord[];
}

export interface AgentWorkspaceState {
  activeConversationId: string | null;
  runs: Record<string, AgentRunState>;
  completed: CompletedRunSnapshot | null;
}
