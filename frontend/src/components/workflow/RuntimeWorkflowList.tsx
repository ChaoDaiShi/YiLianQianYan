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

export interface RuntimeWorkflowListProps {
  onRun: (graph: WorkflowGraphRecord) => void;
  onEdit: (graph: WorkflowGraphRecord) => void;
  onCreate: () => void;
  refreshKey: number;
  selectedGraphId?: string | null;
  compact?: boolean;
}

export default function RuntimeWorkflowList({
  onRun,
  onEdit,
  onCreate,
  refreshKey,
  selectedGraphId,
  compact = false,
}: RuntimeWorkflowListProps) {
  const [graphs, setGraphs] = useState<WorkflowGraphRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    const res = await listWorkflowGraphs();
    if (res.ok) {
      setGraphs(res.data);
      setError("");
    } else {
      setError(res.error || "无法加载运行工作流");
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

  if (loading && graphs.length === 0) {
    return (
      <div className="flex justify-center py-10">
        <Spinner className="w-6 h-6" />
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

  if (graphs.length === 0 && !error) {
    return (
      <EmptyState
        icon={<span className="text-2xl">⚡</span>}
        title="还没有可执行工作流"
        description="创建一个可执行的 DAG 工作流图，然后点击运行。"
        action={
          <Button size="sm" onClick={onCreate}>
            新建工作流
          </Button>
        }
        className="py-6"
      />
    );
  }

  if (compact) {
    return (
      <div className="flex flex-col gap-2">
        {graphs.map((graph) => {
          const isSelected = selectedGraphId === graph.id;
          return (
            <div
              key={graph.id}
              onClick={() => onEdit(graph)}
              className={`group flex flex-col gap-2 rounded-[var(--radius-md)] border p-3 cursor-pointer transition-all ${
                isSelected
                  ? "border-[var(--accent-primary)] bg-[var(--surface-solid)] ring-1 ring-[var(--accent-primary)]/40 shadow-sm"
                  : "border-[var(--border-soft)] bg-[var(--surface-solid)] hover:border-[var(--border)] hover:bg-[var(--surface-hover)]"
              }`}
            >
              <div className="flex items-start justify-between gap-1.5 min-w-0">
                <div className="min-w-0 flex-1">
                  <h4 className="font-semibold text-xs truncate text-[var(--text)]" title={graph.name}>
                    {graph.name}
                  </h4>
                  {graph.description && (
                    <p className="text-[11px] text-[var(--text-secondary)] line-clamp-1 mt-0.5">
                      {graph.description}
                    </p>
                  )}
                </div>
                <Badge tone="default">
                  <span className="text-[10px]">{graph.definition.nodes.length} 节点</span>
                </Badge>
              </div>

              <div className="flex items-center justify-between text-[11px] text-[var(--text-faint)]">
                <span>{kindSummary(graph.definition)}</span>
                <span className="truncate">
                  {new Date(graph.updated_at).toLocaleDateString()}
                </span>
              </div>

              <div
                className="flex items-center gap-1 pt-1.5 border-t border-[var(--border-soft)]/60"
                onClick={(e) => e.stopPropagation()}
              >
                <Button
                  size="sm"
                  variant="primary"
                  className="h-6 text-[11px] px-2"
                  onClick={() => onRun(graph)}
                  title="运行此工作流"
                >
                  <Play className="w-3 h-3" />
                  运行
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  className="h-6 text-[11px] px-2"
                  onClick={() => onEdit(graph)}
                  title="编辑图"
                >
                  <Pencil className="w-3 h-3" />
                  编辑
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-6 text-[11px] px-1.5 ml-auto text-[var(--text-secondary)] hover:text-[var(--danger)]"
                  onClick={() => handleDelete(graph)}
                  title="删除工作流"
                >
                  <Trash2 className="w-3 h-3" />
                </Button>
              </div>
            </div>
          );
        })}
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      {graphs.map((graph) => {
        const isSelected = selectedGraphId === graph.id;
        return (
          <Panel
            key={graph.id}
            className={`flex flex-col gap-2 transition-all ${
              isSelected ? "ring-1 ring-[var(--accent-primary)]" : ""
            }`}
          >
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
        );
      })}
    </div>
  );
}
