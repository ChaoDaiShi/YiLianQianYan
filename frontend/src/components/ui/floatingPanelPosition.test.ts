import { describe, expect, it } from "vitest";
import { placeFloatingPanel } from "./floatingPanelPosition";

const panel = { width: 326, height: 320 };
const desktop = { width: 1280, height: 800 };

describe("floating panel position", () => {
  it("prefers the right of its control, with a left fallback on a right rail", () => {
    expect(placeFloatingPanel({ left: 160, right: 192, top: 728, bottom: 760 }, panel, desktop)).toEqual({ left: 200, top: 440, side: "right" });
    expect(placeFloatingPanel({ left: 1130, right: 1162, top: 728, bottom: 760 }, panel, desktop)).toEqual({ left: 796, top: 440, side: "left" });
  });

  it("keeps a narrow window menu in bounds and below an anchor when above is unavailable", () => {
    expect(placeFloatingPanel({ left: 180, right: 212, top: 48, bottom: 80 }, panel, { width: 390, height: 700 })).toEqual({ left: 52, top: 88, side: "below" });
  });
});
