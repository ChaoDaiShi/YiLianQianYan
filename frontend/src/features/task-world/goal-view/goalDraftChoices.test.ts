import { describe, expect, it } from "vitest";
import type { ProofDraftSummary } from "../../../api/proofloop";
import { confirmedGoalChoices } from "./goalDraftChoices";

const draft = (draftId: string, graphId: string, status: ProofDraftSummary["status"]): ProofDraftSummary => ({
  draft_id: draftId, graph_id: graphId, goal_id: `goal-${graphId}`, user_goal: draftId, revision: 1, status,
});

describe("confirmed goal choices", () => {
  it("excludes unconfirmed plans and lists a shared graph only once", () => {
    const latest = draft("latest", "shared", "confirmed");
    expect(confirmedGoalChoices([draft("draft", "unconfirmed", "proposed"), latest, draft("older", "shared", "confirmed"), draft("second", "other", "confirmed")]))
      .toEqual([latest, draft("second", "other", "confirmed")]);
  });
});
