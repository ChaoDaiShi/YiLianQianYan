import { CircleCheck, Unplug } from "lucide-react";
import ApprovalCard from "../../components/approval/ApprovalCard";
import ToolCallCard from "../../components/chat/ToolCallCard";
import type { ToolCallRecord } from "../../types";
import type { PendingApproval } from "../../types/approval";
import type { ConnectionStatus, ExecutionRecord } from "./model";

interface CurrentActionCardProps {
  record: ExecutionRecord | null;
  connection: ConnectionStatus;
  approval?: PendingApproval;
  resolving?: boolean;
  onApprove: (approval: PendingApproval) => void;
  onReject: (approval: PendingApproval) => void;
}

function displayStatus(record: ExecutionRecord): ToolCallRecord["status"] {
  if (record.approvalStatus === "pending") return "blocked";
  if (record.executionStatus === "running" || record.executionStatus === "queued") {
    return "running";
  }
  if (record.executionStatus === "succeeded") return "success";
  return "error";
}

export default function CurrentActionCard({
  record,
  connection,
  approval,
  resolving = false,
  onApprove,
  onReject,
}: CurrentActionCardProps) {
  if (record?.approvalStatus === "pending" && approval) {
    return (
      <ApprovalCard
        approval={approval}
        resolving={resolving}
        onApprove={onApprove}
        onReject={onReject}
      />
    );
  }

  if (!record) {
    const interrupted = connection === "interrupted" || connection === "error";
    return (
      <div className="rounded-[var(--radius-md)] border border-transparent bg-transparent px-1 py-3">
        <div className="flex items-start gap-3">
          {interrupted ? (
            <Unplug className="mt-0.5 h-4 w-4 shrink-0 text-[var(--danger)]" />
          ) : (
            <CircleCheck className="mt-0.5 h-4 w-4 shrink-0 text-[var(--text-faint)]" />
          )}
          <div>
            <p className="text-sm font-normal text-[var(--text-secondary)]">
              {interrupted ? "执行流已中断" : "当前没有正在执行的任务"}
            </p>
            <p className="mt-1 text-xs leading-5 text-[var(--text-faint)]">
              {interrupted
                ? "连接未正常结束，请检查聊天区提示后重试。"
                : "新的工具调用与审批会显示在这里。"}
            </p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <ToolCallCard
      toolCallId={record.toolCallId}
      name={record.name}
      args={record.args}
      status={displayStatus(record)}
      result={record.result}
      riskLevel={record.riskLevel}
      approvalStatus={record.approvalStatus}
      verificationStatus={record.verificationStatus}
      verificationReason={record.verificationReason}
      startedAt={record.startedAt}
      finishedAt={record.finishedAt}
    />
  );
}
