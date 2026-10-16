import fixture from "../../../backend/tests/fixtures/proofloop/v12/planning-ranked.json";
import { describe, expect, it } from "vitest";
import type { CreateProofDraftRequest } from "./proofloop";

describe("shared v1.2 planning contract fixture", () => {
  it("keeps the same snake_case goal requirements used by the Rust compiler", () => {
    const request = fixture.request as CreateProofDraftRequest;
    expect(request.planning?.strategy).toBe("ranked");
    expect(request.planning?.candidate_count).toBe(fixture.candidates.length);
    expect(request.requirements?.deliverables.map((item) => item.id)).toEqual(fixture.contract.deliverables.map((item: { id: string }) => item.id));
    expect(fixture.expected_compile_statuses).toEqual(["ready", "ready"]);
  });
});
