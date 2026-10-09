import type { ComponentProps } from "react";
import ArtifactResultsPanel from "../../tasks/ArtifactResultsPanel";
import MemorySkillCandidatePanel from "../../memory/MemorySkillCandidatePanel";

type EvidenceSource = ComponentProps<typeof ArtifactResultsPanel>["source"];

interface ArtifactSectionProps {
  source: EvidenceSource;
  completed: boolean;
}

/**
 * What a finished execution produced: its artifacts, and the reviewed
 * memory-to-skill candidate that can be derived from the same evidence.
 *
 * Both panels read one evidence source and are only actionable once the
 * execution has actually succeeded, so they travel together.
 */
export default function ArtifactSection({ source, completed }: ArtifactSectionProps) {
  return (
    <>
      <section className="mt-5 border-t border-[var(--border-soft)] pt-4">
        <ArtifactResultsPanel source={source} completed={completed} />
      </section>
      <section className="mt-5 border-t border-[var(--border-soft)] pt-4">
        <MemorySkillCandidatePanel source={source} completed={completed} />
      </section>
    </>
  );
}
