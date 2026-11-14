import { describe, expect, it } from "vitest";
import { dayStarts } from "#/matches/components/turn_log.ts";
import { awbrnOutline, readArchiveOutline } from "./archive_outline.ts";

const setup = { players: [{ coId: 1 }, { coId: 1 }] };
const build = (player: number) => ({
  player,
  command: { type: "build", position: { x: 2, y: 3 }, unit_type: "infantry" },
});
const end = (player: number) => ({ player, command: { type: "endTurn" } });

describe("archive outline", () => {
  it("turns the day over when the turn passes back to the first seat", () => {
    const outline = awbrnOutline({
      setup,
      actions: [build(0), end(0), build(1), end(1), build(0)],
    });
    expect(outline?.map((boundary) => [boundary.day, boundary.actingSlot])).toEqual([
      [1, null],
      [1, 0],
      [1, 0],
      [1, 1],
      [2, 1],
      [2, 0],
    ]);
    expect(outline?.[1]?.summary).toEqual({
      unit: "infantry",
      verb: "Built",
      tile: { x: 2, y: 3 },
    });
  });

  it("reads an attack from where the unit stands as firing", () => {
    const outline = awbrnOutline({
      setup,
      actions: [
        {
          player: 0,
          command: {
            type: "moveUnit",
            unit_id: 1,
            path: [{ x: 4, y: 4 }],
            action: { type: "attack", target: { x: 5, y: 4 } },
          },
        },
        {
          player: 0,
          command: {
            type: "moveUnit",
            unit_id: 2,
            path: [
              { x: 1, y: 1 },
              { x: 1, y: 2 },
            ],
            action: { type: "wait" },
          },
        },
      ],
    });
    expect(outline?.slice(1).map((boundary) => boundary.summary?.verb)).toEqual(["Fires", "Moves"]);
    expect(outline?.[2]?.summary?.tile).toEqual({ x: 1, y: 2 });
  });

  it("refuses an archive it does not recognize", () => {
    expect(awbrnOutline({ version: 1 })).toBeNull();
    expect(awbrnOutline({ setup, actions: [{ command: {} }] })).toBeNull();
  });

  it("reads every action of an AWBW archive under the turn it was taken in", async () => {
    const unit = { global: { units_name: "Md.Tank", units_x: 7, units_y: 8 } };
    const log = [
      turn(11, 1, [
        { action: "Move", unit },
        { action: "End", note: 's:9:"x";' },
      ]),
      turn(22, 1, [{ action: "Fire", Move: [] }, { action: "End" }]),
      turn(11, 2, [{ action: "Power", powerName: "Forced March" }]),
    ].join("\n");
    const outline = await readArchiveOutline(await storedZip("a1234", await gzip(log)));
    expect(outline?.map((boundary) => [boundary.day, boundary.actingSlot])).toEqual([
      [1, null],
      [1, 11],
      [1, 11],
      [1, 22],
      [2, 22],
      [2, 11],
    ]);
    expect(outline?.map((boundary) => boundary.summary?.verb)).toEqual([
      undefined,
      "Moves",
      "Ends turn",
      "Fires",
      "Ends turn",
      "Forced March",
    ]);
    expect(outline?.[1]?.summary).toEqual({ unit: "md-tank", verb: "Moves", tile: { x: 7, y: 8 } });
    expect(dayStarts(outline!)).toEqual([4]);
  });

  it("places an AWBW capture made without a move on its building", async () => {
    const log = turn(11, 1, [
      { action: "Capt", Move: [], Capt: { buildingInfo: { buildings_x: 3, buildings_y: 5 } } },
    ]);
    const outline = await readArchiveOutline(await storedZip("a1234", await gzip(log)));
    expect(outline?.[1]?.summary).toEqual({ unit: null, verb: "Captures", tile: { x: 3, y: 5 } });
  });
});

/** One AWBW turn line: the player, the day and the actions taken. */
function turn(player: number, day: number, actions: object[]): string {
  const action = (json: object) => {
    const text = JSON.stringify(json);
    return `s:${new TextEncoder().encode(text).length}:"${text}";`;
  };
  return `p:${player};d:${day};a:a:3:{i:0;i:${player};i:1;i:${day};i:2;a:${actions.length}:{${actions
    .map((each, index) => `i:${index};${action(each)}`)
    .join("")}}}`;
}

async function gzip(text: string): Promise<Uint8Array> {
  const stream = new Blob([text]).stream().pipeThrough(new CompressionStream("gzip"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** A one-entry .zip that stores its file as is. */
async function storedZip(name: string, content: Uint8Array): Promise<Uint8Array> {
  const nameBytes = new TextEncoder().encode(name);
  const local = new DataView(new ArrayBuffer(30));
  local.setUint32(0, 0x04034b50, true);
  local.setUint32(18, content.length, true);
  local.setUint32(22, content.length, true);
  local.setUint16(26, nameBytes.length, true);
  const localSize = 30 + nameBytes.length + content.length;
  const central = new DataView(new ArrayBuffer(46));
  central.setUint32(0, 0x02014b50, true);
  central.setUint32(20, content.length, true);
  central.setUint32(24, content.length, true);
  central.setUint16(28, nameBytes.length, true);
  central.setUint32(42, 0, true);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, 0x06054b50, true);
  end.setUint16(8, 1, true);
  end.setUint16(10, 1, true);
  end.setUint32(12, 46 + nameBytes.length, true);
  end.setUint32(16, localSize, true);
  return new Uint8Array(
    await new Blob([
      local.buffer,
      nameBytes,
      content as BlobPart,
      central.buffer,
      nameBytes,
      end.buffer,
    ]).arrayBuffer(),
  );
}
