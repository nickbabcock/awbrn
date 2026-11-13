import { describe, expect, it } from "vitest";
import { buildMenuMount } from "./board_menu_mount.ts";

describe("buildMenuMount", () => {
  it("opens the build menu as a sheet on a phone", () => {
    // A phone has neither the docked column nor the rail, so the sheet is the
    // only place the unit list can be shown.
    expect(buildMenuMount("phone")).toBe("sheet");
  });

  it("docks it beside the board when the table has a column", () => {
    expect(buildMenuMount("dock")).toBe("column");
    expect(buildMenuMount("gather")).toBe("column");
  });

  it("puts it in the rail's View block when the table has no column", () => {
    expect(buildMenuMount("none")).toBe("rail");
  });

  it("waits for the table to be measured", () => {
    expect(buildMenuMount("pending")).toBeNull();
  });
});
