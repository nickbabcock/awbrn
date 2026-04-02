import { existsSync, writeFileSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { randomBytes } from "crypto";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const devVars = join(root, "web", ".dev.vars");

if (!existsSync(devVars)) {
  const secret = randomBytes(32).toString("base64url");
  writeFileSync(devVars, `AUTH_SECRET=${secret}\n`);
  console.log("Created .dev.vars with a generated AUTH_SECRET");
}
