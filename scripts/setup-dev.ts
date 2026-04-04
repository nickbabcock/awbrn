import { existsSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { randomBytes } from "node:crypto";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const devVars = join(root, "web", ".dev.vars");

if (!existsSync(devVars)) {
  const secret = randomBytes(32).toString("base64url");
  writeFileSync(devVars, `AUTH_SECRET=${secret}\n`);
  console.log("Created .dev.vars with a generated AUTH_SECRET");
}
