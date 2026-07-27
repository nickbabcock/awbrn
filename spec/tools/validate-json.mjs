#!/usr/bin/env node

import { readFileSync, readdirSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";

const specRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const schemaRoot = resolve(specRoot, "schema");
const rulesetRoot = resolve(specRoot, "rulesets");
const fixtureRoot = resolve(specRoot, "fixtures");
const schemaBase = "https://raw.githubusercontent.com/nickbabcock/awbrn/master/spec/schema/";

const jsonFiles = (root) => {
  const files = [];
  const visit = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = resolve(directory, entry.name);
      if (entry.isDirectory()) {
        visit(path);
      } else if (entry.name.endsWith(".json")) {
        files.push(path);
      }
    }
  };
  visit(root);
  return files.sort();
};

const load = (path) => JSON.parse(readFileSync(path, "utf8"));
const ajv = new Ajv2020({
  allErrors: true,
  allowUnionTypes: true,
  strict: false,
});

const schemaFiles = jsonFiles(schemaRoot);
for (const path of schemaFiles) {
  const schema = load(path);
  const expectedId = `${schemaBase}${basename(path)}`;
  if (schema.$id !== expectedId) {
    throw new Error(`${path}: expected $id ${expectedId}, found ${schema.$id}`);
  }
  ajv.addSchema(schema);
}

const failures = [];
const validate = (schemaId, path) => {
  const validator = ajv.getSchema(schemaId);
  if (!validator) {
    failures.push(`${path}: schema is not loaded: ${schemaId}`);
    return;
  }
  if (!validator(load(path))) {
    failures.push(`${path}: ${ajv.errorsText(validator.errors, { separator: "; " })}`);
  }
};

let rulesets = 0;
for (const path of jsonFiles(rulesetRoot)) {
  const value = load(path);
  if (typeof value.$schema !== "string") {
    continue;
  }
  validate(`${schemaBase}${basename(value.$schema)}`, path);
  rulesets += 1;
}

const fixtures = jsonFiles(fixtureRoot);
for (const path of fixtures) {
  validate(`${schemaBase}case.schema.json`, path);
}

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exitCode = 1;
} else {
  console.log(
    `valid: ${schemaFiles.length} schemas, ${rulesets} ruleset tables, ${fixtures.length} fixtures`,
  );
}
