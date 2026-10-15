import { afterEach, describe, expect, it, vi } from "vitest";
import { selectProofCandidate } from "./proofloop";

describe("selectProofCandidate", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("posts the candidate and current draft revision to the documented CAS route", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify({ draft_id: "draft-1" }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    const result = await selectProofCandidate("draft/1", { expected_revision: 7, candidate_id: "candidate-b" });

    expect(result.ok).toBe(true);
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/api/task-world/proofloop/drafts/draft%2F1/select-candidate"),
      expect.objectContaining({ method: "POST", body: JSON.stringify({ expected_revision: 7, candidate_id: "candidate-b" }) }),
    );
  });
});
