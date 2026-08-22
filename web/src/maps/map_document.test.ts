/// <reference types="node" />

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import map162795 from "../../../assets/maps/162795.json";
import map178597 from "../../../assets/maps/178597.json";
import { awbwMapDataSchema } from "#/awbw/schemas.ts";
import { importAwbwMapDocument, initSync, WasmMatch } from "#/wasm/awbrn_server.js";
import { importedMapDocumentSchema } from "./map_document.ts";

describe("awbrn map documents", () => {
  initSync({
    module: readFileSync(new URL("../wasm/awbrn_server_bg.wasm", import.meta.url)),
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
});
