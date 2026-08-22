import { useCallback, useEffect, useState } from "react";
import { Play, Pencil, Trash2 } from "lucide-react";
import { Button, Panel, Badge, EmptyState, Spinner } from "../ui";
import {
  listWorkflowGraphs,
  deleteWorkflowGraph,
  type WorkflowGraphRecord,
} from "../../api/client";

const KIND_LABELS: Record<string, string> = {
  agent: "Agent",
  tool: "Tool",
  subagent: "Subagent",
  condition: "Condition",
  output: "Output",
};

function kindSummary(definition: WorkflowGraphRecord["definition"]): string {
  const kinds = new Set(definition.nodes.map((n) => KIND_LABELS[n.kind] || n.kind));
  return Array.from(kinds).join(" / ") || "无节点";
}

export default function RuntimeWorkflowList({
  onRun,
  onEdit,
  onCreate,
  refreshKey,
}: {
  onRun: (graph: WorkflowGraphRecord) => void;
  onEdit: (graph: WorkflowGraphRecord) => void;
  onCreate: () => void;
  refreshKey: number;
}) {
  const [graphs, setGraphs] = useState<WorkflowGraphRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    const res = await listWorkflowGraphs();
    if (res.ok) {
      setGraphs(res.data);
      setError("");
    } else {
      setError(res.error);
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    load();
  }, [load, refreshKey]);

  const handleDelete = async (graph: WorkflowGraphRecord) => {
    if (!window.confirm(`确定删除运行工作流 "${graph.name}" 吗？历史运行记录将保留。`)) return;
    const res = await deleteWorkflowGraph(graph.id);
    if (res.ok) load();
  };

  if (loading) {
    return (
      <div className="flex justify-center py-16">
        <Spinner className="w-8 h-8" />
      </div>
    );
  }

  if (error && graphs.length === 0) {
    return (
      <EmptyState
        icon={<span className="text-2xl">⚡</span>}
        title="无法加载运行工作流"
        description={error}
        action={
          <Button variant="secondary" size="sm" onClick={load}>
            重试
          </Button>
        }
      />
    );
  }

  return (
    <div>
      <div className="grid grid-cols-2 gap-4">
        {graphs.map((graph) => (
          <Panel key={graph.id} className="flex flex-col gap-2">
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0">
                <h3 className="font-semibold text-sm truncate">{graph.name}</h3>
                <p className="text-xs text-[var(--text-muted)] line-clamp-2 mt-0.5">
                  {graph.description || "（无描述）"}
                </p>
              </div>
              <Badge tone="accent">{kindSummary(graph.definition)}</Badge>
            </div>
            <div className="flex items-center gap-3 text-xs text-[var(--text-faint)]">
              <span>{graph.definition.nodes.length} 节点</span>
              <span>{graph.definition.edges.length} 连接</span>
              <span className="truncate">
                更新于 {new Date(graph.updated_at).toLocaleString()}
              </span>
            </div>
            <div className="flex items-center gap-1 mt-1">
              <Button size="sm" variant="primary" onClick={() => onRun(graph)}>
                <Play className="w-3.5 h-3.5" />
                运行
              </Button>
              <Button size="sm" variant="secondary" onClick={() => onEdit(graph)}>
                <Pencil className="w-3.5 h-3.5" />
                编辑
              </Button>
              <Button
                size="sm"
                variant="ghost"
                className="ml-auto"
                onClick={() => handleDelete(graph)}
              >
                <Trash2 className="w-3.5 h-3.5" />
              </Button>
            </div>
          </Panel>
        ))}

        {graphs.length === 0 && !error && (
          <div className="col-span-2">
            <EmptyState
              icon={<span className="text-2xl">⚡</span>}
              title="还没有可执行工作流"
              description="创建一个可执行的 DAG 工作流图，然后点击运行。"
              action={<Button size="sm" onClick={onCreate}>新建工作流</Button>}
            />
          </div>
        )}
      </div>
    </div>
  );
}
