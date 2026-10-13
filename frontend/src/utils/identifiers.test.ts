import { describe, expect, it } from "vitest";
import { formatIdentifier, formatIdentifierText } from "./identifiers";

const uuid = "8e71e495-cc9e-440b-85c9-3a4a69c11af4";

describe("readable identifiers", () => {
  it("uses type prefixes with a stable UUID short code", () => {
    expect(formatIdentifier("artifact", uuid)).toBe("ART-8E71E495");
    expect(formatIdentifier("graph", uuid)).toBe("TG-8E71E495");
    expect(formatIdentifier("goal", `goal-${uuid}`)).toBe("GL-8E71E495");
  });

  it("keeps legacy named identities stable without just cutting their common prefix", () => {
    expect(formatIdentifier("node", "node_report_1")).toBe(formatIdentifier("node", "node_report_1"));
    expect(formatIdentifier("node", "node_report_1")).not.toBe(formatIdentifier("node", "node_report_2"));
    expect(formatIdentifier("node", "node_report_1")).toMatch(/^ND-[0-9A-F]{8}$/);
  });

  it("formats known record identities in explanations and leaves unrelated text intact", () => {
    const text = `产物 '${uuid}' 已生成；大小 797 字节。`;
    expect(formatIdentifierText(text, [{ kind: "artifact", value: uuid }])).toBe("产物 'ART-8E71E495' 已生成；大小 797 字节。");
    expect(uuid).toBe("8e71e495-cc9e-440b-85c9-3a4a69c11af4");
  });

  it("treats named identifiers as literal strings rather than regex patterns", () => {
    expect(formatIdentifierText("node.[1] / node.11", [{ kind: "node", value: "node.[1]" }]))
      .toBe(`${formatIdentifier("node", "node.[1]")} / node.11`);
  });
  it("does not rewrite another record that shares a legacy identifier prefix", () => {
    expect(formatIdentifierText("goal_1 / goal_10", [{ kind: "goal", value: "goal_1" }]))
      .toBe(`${formatIdentifier("goal", "goal_1")} / goal_10`);
  });
});
