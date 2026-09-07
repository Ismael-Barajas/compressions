#!/usr/bin/env node
// Prints the CHANGELOG.md section for the given version (default: package.json's
// version) so the Release workflow can use it as the GitHub release body.
//
//   node scripts/release-notes.mjs          # section for the current version
//   node scripts/release-notes.mjs 1.2.0    # section for a specific version
import { readFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version =
  process.argv[2] ?? JSON.parse(readFileSync(join(root, "package.json"), "utf-8")).version;

const changelog = readFileSync(join(root, "CHANGELOG.md"), "utf-8");
const lines = changelog.split(/\r?\n/);

const headingRe = /^## \[([^\]]+)\]/;
const start = lines.findIndex((l) => headingRe.exec(l)?.[1] === version);
if (start < 0) {
  console.error(`No CHANGELOG.md section found for version ${version}`);
  process.exit(1);
}
let end = lines.findIndex((l, i) => i > start && headingRe.test(l));
if (end < 0) end = lines.length;

// Drop the "## [x.y.z] — date" heading itself; the release title already names the version.
const body = lines.slice(start + 1, end).join("\n").trim();
if (!body) {
  console.error(`CHANGELOG.md section for ${version} is empty`);
  process.exit(1);
}
process.stdout.write(body + "\n");
