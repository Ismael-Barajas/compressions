#!/usr/bin/env node
import { readFileSync, writeFileSync } from "fs";
import { execSync } from "child_process";
import { dirname, join } from "path";
import { fileURLToPath } from "url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const arg = process.argv[2];

if (!arg) {
  console.error("Usage: node scripts/bump-version.mjs <patch|minor|major|x.y.z>");
  process.exit(1);
}

const isBump = ["patch", "minor", "major"].includes(arg);
const isSemver = /^\d+\.\d+\.\d+(-[\w.]+)?$/.test(arg);

if (!isBump && !isSemver) {
  console.error(`Invalid version: ${arg}`);
  process.exit(1);
}

execSync(`npm version ${arg} --no-git-tag-version`, { cwd: root, stdio: "inherit" });

const { version } = JSON.parse(readFileSync(join(root, "package.json"), "utf-8"));

const tauriPath = join(root, "src-tauri/tauri.conf.json");
const tauri = JSON.parse(readFileSync(tauriPath, "utf-8"));
tauri.version = version;
writeFileSync(tauriPath, JSON.stringify(tauri, null, 2) + "\n");

const cargoPath = join(root, "src-tauri/Cargo.toml");
const cargo = readFileSync(cargoPath, "utf-8").replace(
  /version = "[^"]+"/,
  `version = "${version}"`,
);
writeFileSync(cargoPath, cargo);

// Sync Cargo.lock so the release commit doesn't leave it stale
execSync("cargo update -p compressions", { cwd: join(root, "src-tauri"), stdio: "inherit" });

console.log(`\nBumped to v${version}`);
console.log("Next steps:");
console.log("  1. Update CHANGELOG.md");
console.log(`  2. git commit -am "chore: release v${version}"`);
console.log(`  3. git tag v${version} && git push origin v${version}`);
