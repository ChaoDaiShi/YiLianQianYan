import { describe, expect, it } from "vitest";
import { isBinaryPreviewFile } from "./WorkspaceFilesPanel";

describe("workspace file selection", () => {
  it("routes supported documents and media to the right preview pane", () => {
    for (const name of ["报告.DOCX", "scan.pdf", "slides.pptx", "sheet.xls", "old.doc", "notes.rtf", "nested.zip", "photo.png", "sound.wav", "movie.mp4"]) expect(isBinaryPreviewFile(name), name).toBe(true);
  });
  it("keeps text and Markdown in the existing reader", () => {
    for (const name of ["README.md", "src/main.rs", "notes.txt", "data.csv"]) expect(isBinaryPreviewFile(name), name).toBe(false);
  });
});
