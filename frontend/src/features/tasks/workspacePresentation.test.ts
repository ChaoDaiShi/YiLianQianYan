import { describe, expect, it } from "vitest";
import {
  classifyArtifactType,
  formatWorkspacePath,
} from "./workspacePresentation";

describe("workspace presentation", () => {
  it("keeps the workspace name primary and path secondary", () => {
    expect(formatWorkspacePath("F:\\项目开发\\忆涟千言\\YiLianQianYan")).toBe(
      "F:\\项目开发\\忆涟千言\\YiLianQianYan"
    );
    expect(formatWorkspacePath(null)).toBeNull();
  });

  it.each([
    ["README.md", "markdown"],
    ["package.json", "json"],
    ["src/main.ts", "code"],
    ["image.png", "image"],
    ["notes.bin", "file"],
  ] as const)("classifies %s as %s", (name, expected) => {
    expect(classifyArtifactType(name)).toBe(expected);
  });
});
