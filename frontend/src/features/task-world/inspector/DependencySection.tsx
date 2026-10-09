import type { TaskEdge } from "../../../api/taskWorld";
import { Button } from "../../../components/ui";
import type { TaskNodeProjection } from "../taskGraphProjection";

interface DependencySectionProps {
  node: TaskNodeProjection;
  dependencyEdges: TaskEdge[];
  dependencyCandidates: TaskNodeProjection[];
  dependencyTarget: string;
  onDependencyTargetChange: (value: string) => void;
  onAddDependency: (from: string, to: string) => Promise<void>;
  onRemoveDependency: (from: string, to: string) => Promise<void>;
  saving: boolean;
  graphLocked: boolean;
}

/**
 * Incoming edges only — this node's prerequisites.
 *
 * The candidate list excludes the node itself and anything already pointing at
 * it, so the same edge cannot be proposed twice.
 */
export default function DependencySection({
  node,
  dependencyEdges,
  dependencyCandidates,
  dependencyTarget,
  onDependencyTargetChange,
  onAddDependency,
  onRemoveDependency,
  saving,
  graphLocked,
}: DependencySectionProps) {
  const incomingDependencies = dependencyEdges.filter((edge) => edge.to === node.id);

  const addDependency = async () => {
    if (!dependencyTarget || dependencyTarget === node.id) return;
    await onAddDependency(dependencyTarget, node.id);
    onDependencyTargetChange("");
  };

  return (
    <section
      className="mt-5 border-t border-[var(--border-soft)] pt-4"
      aria-labelledby="task-world-dependencies-heading"
    >
      <div className="flex items-center justify-between gap-2">
        <h3
          id="task-world-dependencies-heading"
          className="text-xs font-semibold text-[var(--text)]"
        >
          依赖
        </h3>
        <span className="text-meta text-[var(--text-faint)]">
          {incomingDependencies.length} 条入边
        </span>
      </div>
      {incomingDependencies.length > 0 && (
        <ul className="mt-2 space-y-1">
          {incomingDependencies.map((edge) => (
            <li
              key={`${edge.from}->${edge.to}`}
              className="flex items-center justify-between gap-2 rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-2 py-1.5 text-xs"
            >
              <span className="truncate text-[var(--text-secondary)]">{edge.from}</span>
              <button
                type="button"
                className="shrink-0 text-[var(--text-faint)] hover:text-[var(--danger)]"
                disabled={saving || graphLocked}
                onClick={() => void onRemoveDependency(edge.from, edge.to)}
                aria-label={`删除依赖 ${edge.from}`}
              >
                移除
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="mt-2 flex gap-2">
        <select
          className="min-w-0 flex-1 rounded-[var(--radius-sm)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-2 py-1.5 text-xs text-[var(--text)]"
          aria-label="选择依赖节点"
          value={dependencyTarget}
          onChange={(event) => onDependencyTargetChange(event.target.value)}
        >
          <option value="">添加前置节点</option>
          {dependencyCandidates
            .filter(
              (candidate) =>
                candidate.id !== node.id &&
                !incomingDependencies.some((edge) => edge.from === candidate.id),
            )
            .map((candidate) => (
              <option key={candidate.id} value={candidate.id}>
                {candidate.title}
              </option>
            ))}
        </select>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={!dependencyTarget || saving || graphLocked}
          onClick={() => void addDependency()}
        >
          添加
        </Button>
      </div>
    </section>
  );
}
