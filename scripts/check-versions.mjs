#!/usr/bin/env node
import { readFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf-8")).version;
const tauri = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf-8")).version;
const cargoMatch = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf-8").match(
  /^version = "([^"]+)"/m,
);
const cargo = cargoMatch?.[1];
const lockMatch = readFileSync(join(root, "src-tauri/Cargo.lock"), "utf-8").match(
  /name = "compressions"\nversion = "([^"]+)"/,
);
const cargoLock = lockMatch?.[1];

const versions = {
  "package.json": pkg,
  "tauri.conf.json": tauri,
  "Cargo.toml": cargo,
  "Cargo.lock": cargoLock,
};

const unique = new Set(Object.values(versions));
if (unique.size !== 1 || !cargo || !cargoLock) {
  console.error("Version mismatch across manifests:");
  for (const [file, v] of Object.entries(versions)) {
    console.error(`  ${file}: ${v ?? "(missing)"}`);
  }
  process.exit(1);
}

console.log(`Versions in sync: ${pkg}`);
