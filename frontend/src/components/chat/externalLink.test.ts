import { describe, expect, it } from "vitest";
import { isExternalHttpUrl } from "./externalLink";

describe("external link detection", () => {
  it("recognizes absolute HTTP(S) links as external browser targets", () => {
    expect(isExternalHttpUrl("https://www.deepseek.com")).toBe(true);
    expect(isExternalHttpUrl("http://localhost:1420/docs")).toBe(true);
  });

  it("does not treat in-app or unsafe schemes as external browser targets", () => {
    expect(isExternalHttpUrl("/settings")).toBe(false);
    expect(isExternalHttpUrl("#history")).toBe(false);
    expect(isExternalHttpUrl("javascript:alert(1)")).toBe(false);
    expect(isExternalHttpUrl("mailto:user@example.com")).toBe(false);
  });
});
