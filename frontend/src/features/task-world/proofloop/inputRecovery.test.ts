import { describe, expect, it } from "vitest";
import * as model from "./proofloopModel";
import type { CandidatePlan, PendingInput } from "../../../api/proofloop";

describe("fixed parameter recovery", () => {
  it("replaces the unresolved binding without changing other inputs or mutating the draft", () => {
    const plan: CandidatePlan = { schema_version: 1, nodes: [{ id: "consume", title: "Consume", instruction: "Consume", capability_id: "mcp.consume", literal_input: { title: "Report" }, bindings: [{ target_path: ["sources"], source: { kind: "dependency_output", node_id: "fetch", path: ["structured", "sources"] } }] }], edges: [{ from: "fetch", to: "consume" }], deliverable_nodes: [] };
    const pending: PendingInput = { node_id: "consume", target_path: ["sources"], reason: "unknown_output" };
    const recover = (model as unknown as { withFixedInput: (plan: CandidatePlan, pending: PendingInput, value: unknown) => CandidatePlan }).withFixedInput;
    const updated = recover(plan, pending, ["confirmed source"]);
    expect(updated.nodes[0].literal_input.sources).toEqual(["confirmed source"]);
    expect(updated.nodes[0].literal_input.title).toBe("Report");
    expect(updated.nodes[0].bindings).toEqual([]);
    expect(plan.nodes[0].bindings).toHaveLength(1);
    expect(updated.edges).toEqual(plan.edges);
  });
});
