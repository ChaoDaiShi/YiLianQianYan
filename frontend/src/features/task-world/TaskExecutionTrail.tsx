import {
  buildExecutionTrail,
  executionStatusLabel,
  type TaskGraphProjection,
} from "./taskGraphProjection";

interface TaskExecutionTrailProps {
  projection: TaskGraphProjection;
  focusedNodeId: string | null;
  onFocus: (nodeId: string) => void;
  onLocate: (nodeId: string) => void;
}

export default function TaskExecutionTrail({ projection, focusedNodeId, onFocus, onLocate }: TaskExecutionTrailProps) {
  const items = buildExecutionTrail(projection);

  return (
    <aside className="task-world-trail" aria-label="执行轨迹">
      <div className="task-world-trail-heading">
        <span>Execution Trail</span>
        <span>r{projection.revision}</span>
      </div>
      <div className="task-world-trail-list">
        {items.map((item) => (
          <div className="task-world-trail-item" key={item.nodeId} data-node-id={item.nodeId}>
          <button
            key={item.nodeId}
            type="button"
            className={focusedNodeId === item.nodeId ? "is-active" : ""}
            onClick={() => onFocus(item.nodeId)}
          >
            <span className="task-world-trail-dot" data-status={item.node.status} />
            <span className="min-w-0 flex-1 truncate">{item.node.title}</span>
            {(() => {
              const execution_status = item.node.latest_execution?.status;
              return (
                <span className="task-world-trail-status" data-execution-status={execution_status || undefined}>
                  {execution_status
                    ? `${executionStatusLabel(execution_status)} · #${item.node.latest_execution?.attempt}`
                    : trailStatus(item.node.status)}
                </span>
              );
            })()}
          </button>
          <button type="button" className="task-world-trail-locate" aria-label={`定位节点：${item.node.title}`} onClick={() => onLocate(item.nodeId)}>⌖</button>
          </div>
        ))}
      </div>
    </aside>
  );
}

function trailStatus(status: string): string {
  if (status === "succeeded") return "✓";
  if (status === "running") return "●";
  if (status === "failed" || status === "blocked") return "!";
  return "○";
}
