import { describe, expect, it } from "vitest";
import { actionDay, dayLines, dayStarts, logDays, type LogBoundary } from "./turn_log.ts";

/** Two seats over two days: seat 0 acts twice and ends, seat 1 acts once and ends. */
const outline: LogBoundary[] = [
  { day: 1, actingSlot: null },
  { day: 1, actingSlot: 0 },
  { day: 1, actingSlot: 0 },
  { day: 1, actingSlot: 0 },
  { day: 1, actingSlot: 1 },
  { day: 2, actingSlot: 1 },
  { day: 2, actingSlot: 0 },
];

describe("turn log", () => {
  it("files the action that ends a day under the day it was taken on", () => {
    expect(actionDay(outline, 5)).toBe(1);
    expect(actionDay(outline, 6)).toBe(2);
    expect(actionDay(outline, 0)).toBe(1);
  });

  it("numbers a day's actions from one", () => {
    const lines = dayLines(outline, 1);
    expect(lines.map((line) => [line.index, line.number, line.actingSlot])).toEqual([
      [1, 1, 0],
      [2, 2, 0],
      [3, 3, 0],
      [4, 4, 1],
      [5, 5, 1],
    ]);
    expect(dayLines(outline, 2).map((line) => line.index)).toEqual([6]);
  });

  it("lists the days that hold actions", () => {
    expect(logDays(outline)).toEqual([1, 2]);
    expect(logDays([{ day: 1, actingSlot: null }])).toEqual([]);
  });

  it("ticks the boundary where each new day begins", () => {
    expect(dayStarts(outline)).toEqual([5]);
  });
});
