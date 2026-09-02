import { describe, expect, it } from "vitest";
import { boardOwnsKey, canvasPhysicalSize } from "./dom_transport";

describe("canvasPhysicalSize", () => {
  it("uses a plausible device-pixel content box", () => {
    expect(canvasPhysicalSize(390, 260, 3, { inlineSize: 1170, blockSize: 780 })).toEqual({
      width: 1170,
      height: 780,
      scaleFactor: 3,
    });
  });

  it("rejects an emulated device-pixel box reported in CSS pixels", () => {
    expect(canvasPhysicalSize(390, 260, 3, { inlineSize: 390, blockSize: 260 })).toEqual({
      width: 1170,
      height: 780,
      scaleFactor: 3,
    });
  });

  it("allows one-pixel device rounding", () => {
    expect(canvasPhysicalSize(390.4, 260.4, 2, { inlineSize: 781, blockSize: 521 })).toEqual({
      width: 781,
      height: 521,
      scaleFactor: 2,
    });
  });
});

describe("boardOwnsKey", () => {
  it("keeps the keys the board plays with away from the page", () => {
    for (const key of ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", " ", "Backspace"]) {
      expect(boardOwnsKey({ key, shiftKey: false })).toBe(true);
    }
    expect(boardOwnsKey({ key: "Tab", shiftKey: false })).toBe(true);
  });

  it("leaves Shift+Tab as the way out of the board", () => {
    expect(boardOwnsKey({ key: "Tab", shiftKey: true })).toBe(false);
  });

  it("leaves every other key to the page", () => {
    for (const key of ["q", "e", "Enter", "Escape", "F5", "a"]) {
      expect(boardOwnsKey({ key, shiftKey: false })).toBe(false);
    }
  });
});
