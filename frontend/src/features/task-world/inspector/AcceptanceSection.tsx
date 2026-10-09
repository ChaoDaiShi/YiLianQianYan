import { Textarea } from "../../../components/ui";

interface AcceptanceSectionProps {
  acceptanceCriteria: string;
  onAcceptanceCriteriaChange: (value: string) => void;
}

/**
 * How completion is judged — one criterion per line.
 *
 * Kept separate from `BasicSection` because the two are not adjacent in the
 * form: the executor fields sit between them, and merging would have reordered
 * the existing layout.
 */
export default function AcceptanceSection({
  acceptanceCriteria,
  onAcceptanceCriteriaChange,
}: AcceptanceSectionProps) {
  return (
    <Textarea
      label="验收标准（每行一条）"
      value={acceptanceCriteria}
      onChange={(event) => onAcceptanceCriteriaChange(event.target.value)}
      rows={4}
    />
  );
}
