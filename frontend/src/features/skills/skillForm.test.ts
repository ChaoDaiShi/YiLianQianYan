import { describe, expect, it } from "vitest";
import { validateSkillDraft } from "./skillForm";

describe("skill form", () => {
  it("rejects unsafe or empty skill drafts", () => {
    expect(validateSkillDraft({ name: "../x", content: "# X" }).ok).toBe(false);
    expect(validateSkillDraft({ name: "nested/x", content: "# X" }).ok).toBe(false);
    expect(validateSkillDraft({ name: "demo", content: "" }).ok).toBe(false);
  });

  it("normalizes a valid draft without changing its markdown", () => {
    expect(validateSkillDraft({ name: " demo ", content: "# Demo\n\nDo work." })).toEqual({
      ok: true,
      value: { name: "demo", content: "# Demo\n\nDo work." },
    });
  });
});
