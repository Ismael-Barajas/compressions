import { scanPaths } from "./commands";
import { getFileName, getParentDir } from "./fileUtils";
import { useCompressionStore } from "../stores/compressionStore";

/**
 * Expand dropped/picked paths (folders are scanned recursively) into the files to
 * queue. Files found *inside* a folder are skipped when they look like this app's
 * own earlier output, so adding the same folder twice does not compress
 * `photo_compressed.jpg` into `photo_compressed_compressed.jpg`:
 * - anything directly inside a folder named like the output subfolder, and
 * - files whose name ends with the template's fixed suffix (`{name}_compressed`
 *   → `_compressed`, also `_compressed_2` from conflict renames).
 * Paths the user picked or dropped directly are always kept.
 */
export async function scanForQueue(paths: string[]): Promise<string[]> {
  const resolved = await scanPaths(paths);
  const picked = new Set(paths.map((p) => p.toLowerCase()));
  const { subfolderName, outputTemplate } = useCompressionStore.getState();
  const subfolder = subfolderName.trim().toLowerCase() || "compressed";
  const suffix = templateSuffix(outputTemplate);

  const kept = resolved.filter((path) => {
    if (picked.has(path.toLowerCase())) return true;
    const inOutputFolder = getFileName(getParentDir(path)).toLowerCase() === subfolder;
    const hasOutputSuffix = suffix !== null && suffix.test(stem(path));
    return !inOutputFolder && !hasOutputSuffix;
  });

  const skipped = resolved.length - kept.length;
  if (skipped > 0) {
    console.info(`Skipped ${skipped} file(s) that look like earlier compressed output`);
  }
  return kept;
}

function stem(path: string): string {
  const name = getFileName(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

/** `{name}<fixed text>` → a matcher for names ending in that text (plus an optional
 * `_N` conflict suffix). Templates with other tokens or no fixed suffix → null. */
function templateSuffix(template: string): RegExp | null {
  if (!template.startsWith("{name}")) return null;
  const rest = template.slice("{name}".length);
  if (rest === "" || rest.includes("{")) return null;
  const escaped = rest.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`${escaped}(_\\d+)?$`, "i");
}
