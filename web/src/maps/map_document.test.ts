/// <reference types="node" />

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import map162795 from "../../../assets/maps/162795.json";
import map178597 from "../../../assets/maps/178597.json";
import { awbwMapDataSchema } from "#/awbw/schemas.ts";
import { canonicalizeAwbwMap, ensureServerWasmInitialized } from "#/server_wasm.ts";
import { importedMapDocumentSchema } from "./map_document.ts";

describe("awbrn map documents", () => {
  ensureServerWasmInitialized(
    readFileSync(new URL("../wasm/awbrn_server_bg.wasm", import.meta.url)),
  );
  it.each([
    [
      map162795,
      "796d9e654604bae8b8dd6a946c07f0b83d500c6527f0d8ce40ac9117228b869b",
      "93836ec25c9aa17ab5d8092ce1a7fcb65073b629cdcc141aeafb36a488e52183",
      "14582243c58918a48ed9e66457eb2426b10ac145026c2401fe91f897c6237900",
    ],
    [
      map178597,
      "5e7fa0a76a0b35b69933f6fd239d172fc7078428123728304b2d8b11389ed885",
      "171f5ea00586c5990c5a2d05ad4287fb277f63de6e07f3ea95ee2def4bf9d250",
      "626e894f8bf2a864bfc9b151b8d3053f7aa43c3c98b3a0958e7dfba1527b5776",
    ],
  ])("matches the Rust golden digests", (source, contentHash, propertySignature, unitSignature) => {
    const imported = importedMapDocumentSchema.parse(
      canonicalizeAwbwMap(awbwMapDataSchema.parse(source)),
    );
    expect(imported).toMatchObject({
      contentHash,
      propertySignature,
      unitSignature,
    });
  });
});
