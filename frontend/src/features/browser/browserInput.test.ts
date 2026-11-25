import { describe, expect, it } from "vitest";
import * as policy from "./browserPolicy";

describe("browser URL or search input", () => {
  it.each([
    ["https://example.com/a?q=1", "https://example.com/a?q=1"],
    ["example.com/path", "https://example.com/path"],
    ["localhost:3000/path", "http://localhost:3000/path"],
    ["127.0.0.1:5173", "http://127.0.0.1:5173/"],
    ["devbox:8080/app", "http://devbox:8080/app"],
    ["[::1]:3000", "http://[::1]:3000/"],
    ["example.com/@team", "https://example.com/@team"],
    ["example.com/?email=user@example.com", "https://example.com/?email=user@example.com"],
    ["example.com/#@team", "https://example.com/#@team"],
    ["localhost:3000/@team", "http://localhost:3000/@team"],
    ["localhost:3000/?email=user@example.com", "http://localhost:3000/?email=user@example.com"],
    ["localhost:3000/#@team", "http://localhost:3000/#@team"],
  ])("navigates %s directly", (input, expected) => {
    expect(policy.resolveBrowserInput(input)).toBe(expected);
  });

  it("sends actual keywords to the visibly declared Bing destination", () => {
    const url = new URL(policy.resolveBrowserInput("忆涟 浏览器 + C++ & tools"));
    expect(url.origin + url.pathname).toBe("https://www.bing.com/search");
    expect(url.searchParams.get("q")).toBe("忆涟 浏览器 + C++ & tools");
    expect([...url.searchParams.keys()]).toEqual(["q"]);
  });

  it.each(["", "  ", "javascript:alert(1)", "file:///C:/Windows", "data:text/html,hello", "ftp://example.com", "https://user:pass@example.com", "user:pass@example.com", "user@example.com", "https://example.com/\nnext"])("rejects disallowed input instead of searching it: %s", input => {
    expect(() => policy.resolveBrowserInput(input)).toThrow();
  });

  it("keeps saved-site input separate from keyword search", () => {
    expect(() => policy.normalizeBrowserAddress("search for project")).toThrow();
  });
});
