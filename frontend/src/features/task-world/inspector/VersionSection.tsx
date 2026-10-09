import type { TaskCheckpointSummary, TaskRevisionSummary } from "../../../api/taskWorld";
import { Button } from "../../../components/ui";

interface VersionSectionProps {
  revisions: TaskRevisionSummary[];
  checkpoints: TaskCheckpointSummary[];
  restoringCheckpoint: string;
  onRestoringCheckpointChange: (value: string) => void;
  onCheckpoint: () => Promise<void>;
  onRestore: (checkpointId: string) => Promise<void>;
  saving: boolean;
  graphLocked: boolean;
}

/** Graph revisions and the checkpoints that can be restored from. */
export default function VersionSection({
  revisions,
  checkpoints,
  restoringCheckpoint,
  onRestoringCheckpointChange,
  onCheckpoint,
  onRestore,
  saving,
  graphLocked,
}: VersionSectionProps) {
  return (
    <section
      className="mt-5 border-t border-[var(--border-soft)] pt-4"
      aria-labelledby="task-world-history-heading"
    >
      <div className="flex items-center justify-between gap-2">
        <h3 id="task-world-history-heading" className="text-xs font-semibold text-[var(--text)]">
          版本与检查点
        </h3>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={saving || graphLocked}
          onClick={() => void onCheckpoint()}
        >
          建立检查点
        </Button>
      </div>
      <ul className="mt-2 space-y-1.5 text-xs">
        {revisions.slice(-4).reverse().map((revision) => (
          <li
            key={`${revision.graph_id}-${revision.revision}`}
            className="rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-2 py-1.5"
          >
            <span className="font-mono text-[var(--text)]">r{revision.revision}</span>
            <span className="ml-2 text-[var(--text-secondary)]">{revision.change}</span>
          </li>
        ))}
      </ul>
      {checkpoints.length > 0 && (
        <div className="mt-3 flex gap-2">
          <select
            className="min-w-0 flex-1 rounded-[var(--radius-sm)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-2 py-1.5 text-xs text-[var(--text)]"
            aria-label="选择检查点"
            value={restoringCheckpoint}
            onChange={(event) => onRestoringCheckpointChange(event.target.value)}
          >
            <option value="">选择检查点恢复</option>
            {checkpoints.map((checkpoint) => (
              <option key={checkpoint.checkpoint_id} value={checkpoint.checkpoint_id}>
                r{checkpoint.graph_revision} · {checkpoint.checkpoint_id.slice(0, 8)}
              </option>
            ))}
          </select>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={!restoringCheckpoint || saving || graphLocked}
            onClick={() => void onRestore(restoringCheckpoint)}
          >
            恢复
          </Button>
        </div>
      )}
    </section>
  );
}
