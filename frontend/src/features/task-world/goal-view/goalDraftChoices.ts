import type { ProofDraftSummary } from "../../../api/proofloop";

/** A graph may have several saved draft revisions; it remains one task. */
export function confirmedGoalChoices(drafts: ProofDraftSummary[]): ProofDraftSummary[] {
  const choices = new Map<string, ProofDraftSummary>();
  for (const draft of drafts) {
    if (draft.status === "confirmed" && !choices.has(draft.graph_id)) choices.set(draft.graph_id, draft);
  }
  return [...choices.values()];
}
