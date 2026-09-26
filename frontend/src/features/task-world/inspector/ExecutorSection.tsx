import { Input } from "../../../components/ui";
import type { ExecutorAvailability } from "../taskGraphProjection";

interface ExecutorSectionProps {
  executorRef: string;
  onExecutorRefChange: (value: string) => void;
  availability: ExecutorAvailability;
}

/**
 * Who runs the node. The availability readout is advisory only — the backend
 * remains the authority on whether an executor may actually be started.
 */
export default function ExecutorSection({
  executorRef,
  onExecutorRefChange,
  availability,
}: ExecutorSectionProps) {
  return (
    <>
      <Input
        label="执行引用"
        value={executorRef}
        onChange={(event) => onExecutorRefChange(event.target.value)}
        placeholder="例如 skill://research 或 command://…"
      />
      <div className="rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-muted)] px-3 py-2 text-xs">
        <span className="text-[var(--text-faint)]">执行能力</span>
        <p
          className={
            availability.kind === "unavailable"
              ? "mt-1 text-[var(--warning-fg)]"
              : "mt-1 text-[var(--text-secondary)]"
          }
        >
          {availability.label}
        </p>
        <p className="mt-1 break-words text-[var(--text-faint)]">{availability.reason}</p>
      </div>
    </>
  );
}
