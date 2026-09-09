/// <reference types="node" />

import { describe, expect, it } from "vitest";
import map162795 from "../../../assets/maps/162795.json";
import map178597 from "../../../assets/maps/178597.json";
import { awbwMapDataSchema } from "#/awbw/schemas.ts";
import { importAwbwMapDocument, initSync, WasmMatch } from "#/wasm/awbrn_server.js";
import serverWasmModule from "#/wasm/awbrn_server_bg.wasm";
import { awbrnMapDocumentSchema, importedMapDocumentSchema } from "./map_document.ts";
import { mapSaveRequestSchema } from "./schemas.ts";

describe("awbrn map documents", () => {
  initSync({
    module: serverWasmModule,
  });

  it.each([
    [
      map162795,
      "be64764fdc31f5678b311b1e2bc33481bf9be9bdb293f3a0d9987429bf477fde",
      "880c0f66e63fc0779cd7ab9a39b0a792c5ae558e0eaeb66762f1935ad57d327f",
      "544cbe32215ef3182757aa0d05ce4c30b23b2d2e18cd5096682a97c655df4fcc",
    ],
    [
      map178597,
      "dd00fba3fb8ba692b778b01ada39ddc0673ae654e732d24490f1f0515303ad40",
      "20afe95cf6626b44b594a976c20bfce6db81827d98f3695b12792f745298d21e",
      "55453914790832d66556ca34be53389b5c2ccd13decc358e27f13d81dff76b6b",
    ],
  ])("matches the Rust golden digests", (source, contentHash, propertySignature, unitSignature) => {
    const imported = importedMapDocumentSchema.parse(
      importAwbwMapDocument(awbwMapDataSchema.parse(source)),
    );
    expect(imported).toMatchObject({
      contentHash,
      propertySignature,
      unitSignature,
    });
  });

  it("starts a match from a canonical map with predeployed units", () => {
    const { document } = importedMapDocumentSchema.parse(
      importAwbwMapDocument(awbwMapDataSchema.parse(map178597)),
    );

    expect(document.units.length).toBeGreaterThan(0);
    expect(
      () =>
        new WasmMatch({
          map: document,
          players: [
            { factionId: 1, team: null, startingFunds: 0, coId: 1 },
            { factionId: 2, team: null, startingFunds: 0, coId: 2 },
          ],
          fogEnabled: false,
          startingFunds: 0,
        }),
    ).not.toThrow();
  });

  it("accepts a draft with no player seats", () => {
    const document = {
      map_format: 1,
      width: 5,
      height: 5,
      terrain: Array.from({ length: 25 }, () => 1),
      units: [],
      metadata: { name: "Draft", author: "Map maker", player_count: 0 },
    };

    expect(awbrnMapDocumentSchema.parse(document)).toEqual(document);
    expect(mapSaveRequestSchema.parse({ name: "Draft", document })).toMatchObject({ document });
  });

  it("requires the revision an edit was based on", () => {
    const document = {
      map_format: 1 as const,
      width: 5,
      height: 5,
      terrain: Array.from({ length: 25 }, () => 1),
      units: [],
      metadata: { name: "Draft", author: "Map maker", player_count: 0 },
    };
    const request = {
      mapId: "aaaaaaaaaaaa",
      expectedName: "Draft",
      name: "Draft",
      document,
    };

    expect(mapSaveRequestSchema.safeParse({ ...request, expectedRevision: 1 }).success).toBe(true);
    expect(mapSaveRequestSchema.safeParse(request).success).toBe(false);
    expect(
      mapSaveRequestSchema.safeParse({
        name: "Draft",
        expectedRevision: 1,
        expectedName: "Draft",
        document,
      }).success,
    ).toBe(false);
  });
});
