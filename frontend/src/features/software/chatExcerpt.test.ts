import {describe, expect, it} from "vitest";
import {chatExcerptDraft, chatSelection, suggestChatSelection} from "./chatExcerpt";

function windowPixels(hasEditor: boolean) {
  const width = 400, height = 260, data = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const offset = (y * width + x) * 4;
      const shade = x < 130 ? 225 : (hasEditor && y === 190 ? 178 : 250);
      data.set([shade, shade, shade, 255], offset);
    }
  }
  return {width, height, data};
}

describe("chat excerpt", () => {
  it("keeps the suggested area inside the chat and above the editor", () => {
    const area = suggestChatSelection(windowPixels(true))!;
    expect(area.x * 400).toBeGreaterThan(130);
    expect((area.y + area.height) * 260).toBeLessThan(190);
    expect(suggestChatSelection(windowPixels(false))).toBeNull();
  });
  it("accepts a reverse drag and quotes all recognized lines", () => {
    expect(chatSelection({x: 1, y: .75}, {x: .25, y: .25})).toEqual({x: .25, y: .25, width: .75, height: .5});
    expect(chatExcerptDraft("Group\r\nhello\n\nworld")).toMatch(/> Group\n> hello\n> \n> world$/);
  });
});
