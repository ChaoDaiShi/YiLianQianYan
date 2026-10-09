import { memo, useContext } from "react";
import { LogIn, LogOut, Workflow, ShieldCheck, UserRound, Plug } from "lucide-react";
import { EntryNodesContext, nodePresentation } from "./nodePresentation";
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
  const entryNodes = useContext(EntryNodesContext);
  const appearance = nodePresentation(data.task, entryNodes.has(data.task.id));
  const Icon = appearance.kind === "entry" ? LogIn : appearance.kind === "output" ? LogOut : appearance.kind === "approval" ? ShieldCheck : appearance.kind === "human" ? UserRound : appearance.kind === "mcp" ? Plug : Workflow;
  return (
    <div
      className={`task-world-node studio-node ${selected ? "is-selected" : ""}`}
      data-role={data.role}
      data-node-style={appearance.kind}
      data-status={data.status}
      data-execution-status={data.executionStatus || undefined}
      data-running={data.isRunning ? "true" : "false"}
    >
      <Handle type="target" position={Position.Left} className="task-world-handle" />
      <div className="studio-node-heading">
        <span className="studio-node-icon" aria-hidden="true"><Icon size={16}/></span>
        <div className="task-world-node-title" title={data.task.title}>{data.task.title}</div>
      </div>
      <div className="task-world-node-kicker"><span>{appearance.label}</span><span>{appearance.executor}</span></div>
      <div className="task-world-node-summary" title={data.task.instruction_summary}>{data.task.instruction_summary || "添加任务说明…"}</div>
      <div className="task-world-node-execution">
        <span className="studio-node-status">{data.executionStatus ? executionStatusLabel(data.executionStatus) : statusLabel(data.status)}</span>
        <span>{data.task.latest_execution ? `第 ${data.task.latest_execution.attempt} 次` : "尚未执行"}</span>
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
