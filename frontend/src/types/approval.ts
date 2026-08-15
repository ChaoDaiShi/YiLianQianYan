// ============================================================
// Approval types — mirrors backend PendingApproval
// ============================================================

export type RiskLevel = "low" | "medium" | "high" | "critical";

export type ApprovalStatus =
  | "pending"
  | "approved"
  | "rejected"
  | "cancelled"
  | "expired";

export interface PendingApproval {
  approval_id: string;
  conversation_id: string;
  tool_call_id: string;
  tool_name: string;
  arguments: Record<string, unknown>;
  risk_level: RiskLevel;
  reason: string;
  status: ApprovalStatus;
  created_at: string;
  expires_at: string;
  execution_id?: string | null;
  workflow_run_id?: string | null;
  workflow_node_id?: string | null;
}
