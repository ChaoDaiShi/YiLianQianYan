import type { TaskNodeProjection } from "../taskGraphProjection";

interface StateSectionProps {
  node: TaskNodeProjection;
}

/** The projected semantic state of the node: summary, result, error, resources. */
export default function StateSection({ node }: StateSectionProps) {
  return (
    <section
      className="mt-5 border-t border-[var(--border-soft)] pt-4"
      aria-labelledby="task-world-state-heading"
    >
      <h3 id="task-world-state-heading" className="text-xs font-semibold text-[var(--text)]">
        状态与结果
      </h3>
      <p className="mt-2 text-xs leading-5 text-[var(--text-secondary)]">{node.instruction_summary}</p>
      {node.result_summary && (
        <p className="mt-2 rounded-[var(--radius-md)] bg-[var(--success-soft)] px-3 py-2 text-xs text-[var(--success-fg)]">
          结果：{node.result_summary}
        </p>
      )}
      {node.state.error && (
        <p className="mt-2 rounded-[var(--radius-md)] bg-[var(--danger-soft)] px-3 py-2 text-xs text-[var(--danger-fg)]">
          错误：{node.state.error}
        </p>
      )}
      {node.resources.length > 0 && (
        <div className="mt-3">
          <p className="text-meta text-[var(--text-faint)]">资源引用</p>
          <ul className="mt-1 space-y-1 text-xs text-[var(--text-secondary)]">
            {node.resources.map((resource) => (
              <li key={resource.id} className="truncate">
                {resource.name || resource.id}
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
