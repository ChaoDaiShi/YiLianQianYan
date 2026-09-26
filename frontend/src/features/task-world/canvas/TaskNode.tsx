import { memo } from "react";
import { Handle, Position, type NodeProps } from "@xyflow/react";
import {
  executionStatusLabel,
  type TaskGraphCanvasNode,
} from "../taskGraphProjection";

/**
 * One task node on the canvas.
 *
 * Presentation only: everything it shows comes from the projection, and it
 * carries no handlers of its own — selection and focus belong to the surface.
 * The `data-*` attributes are the contract the canvas styling and the
 * execution-status assertions read.
 */
function TaskNode({ data, selected }: NodeProps<TaskGraphCanvasNode>) {
  return (
    <div
      className={`task-world-node ${selected ? "is-selected" : ""}`}
      data-role={data.role}
      data-status={data.status}
      data-execution-status={data.executionStatus || undefined}
      data-running={data.isRunning ? "true" : "false"}
    >
      <Handle type="target" position={Position.Left} className="task-world-handle" />
      <div className="task-world-node-kicker">
        <span>{data.role}</span>
        <span>{data.executionStatus ? executionStatusLabel(data.executionStatus) : statusLabel(data.status)}</span>
      </div>
      <div className="task-world-node-title">{data.task.title}</div>
      <div className="task-world-node-summary">{data.task.instruction_summary}</div>
      <div className="task-world-node-execution">
        <span>{data.task.latest_execution ? `第 ${data.task.latest_execution.attempt} 次尝试` : "\u00a0"}</span>
        <span>{data.isRunning ? "Running" : "\u00a0"}</span>
      </div>
      <Handle type="source" position={Position.Right} className="task-world-handle" />
    </div>
  );
}

export default memo(TaskNode);

function statusLabel(status: TaskGraphCanvasNode["data"]["status"]): string {
  const labels: Record<TaskGraphCanvasNode["data"]["status"], string> = {
    pending: "待处理",
    runnable: "可运行",
    running: "运行中",
    succeeded: "已完成",
    failed: "失败",
    blocked: "已阻塞",
    cancelled: "已取消",
    invalidated: "需复核",
  };
  return labels[status];
}
