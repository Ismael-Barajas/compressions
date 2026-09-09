import type { AudioCompressionFormat, MediaType, OutputMode, QueuedFile } from "../types/compression";
import { mediaTypeForExtension } from "./mediaTypes";

export function getMediaType(filePath: string): MediaType | null {
  const dot = filePath.lastIndexOf(".");
  if (dot < 0) return null;
  return mediaTypeForExtension(filePath.slice(dot));
}

/** Index of the last `/` or `\` in `path` (-1 if neither); handles mixed separators. */
function lastSeparatorIndex(path: string): number {
  return Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
}

/** Separator to join with: whichever appears last in `path` (default `/`). */
function joinSeparator(path: string): string {
  return path.lastIndexOf("\\") > path.lastIndexOf("/") ? "\\" : "/";
}

/** Strip path separators and `..` so a value can only ever name a single
 * file/folder inside the intended directory. */
function sanitizePathSegment(value: string): string {
  return value.replace(/[/\\]/g, "_").replace(/\.\./g, "_");
}

export function getFileName(filePath: string): string {
  return filePath.slice(lastSeparatorIndex(filePath) + 1) || filePath;
}

export function formatFileSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const k = 1024;
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), units.length - 1);
  const size = bytes / Math.pow(k, i);
  return `${size.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

export function getSavingsPercent(inputSize: number, outputSize: number): number {
  if (inputSize === 0) return 0;
  return Math.round(((inputSize - outputSize) / inputSize) * 100);
}

/** Values for the `{date}` / `{time}` template tokens. Compute once per batch so
 * every file in the batch shares the same stamp. */
export function templateStamp(now: Date = new Date()): { date: string; time: string } {
  // Local date, matching the local time below (toISOString would give UTC and
  // roll the date over near midnight).
  const pad = (n: number) => String(n).padStart(2, "0");
  const date = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
  const time = `${pad(now.getHours())}-${pad(now.getMinutes())}-${pad(now.getSeconds())}`;
  return { date, time };
}

export function getOutputFileName(
  inputPath: string,
  format?: string,
  template: string = "{name}_compressed",
  stamp: { date: string; time: string } = templateStamp(),
): string {
  const fileName = getFileName(inputPath);
  const dotIndex = fileName.lastIndexOf(".");
  const name = dotIndex >= 0 ? fileName.slice(0, dotIndex) : fileName;
  const inputExt = dotIndex >= 0 ? fileName.slice(dotIndex).toLowerCase() : "";
  // GIF inputs always output as GIF to preserve animation
  const ext = inputExt === ".gif"
    ? ".gif"
    : format
      ? `.${format.toLowerCase()}`
      : inputExt;

  // Both the file stem and the user's template are sanitized so neither can
  // escape the output directory. `{name}` uses a function replacer so `$&`-style
  // patterns in a file name are inserted literally.
  const safeName = sanitizePathSegment(name);
  const safeTemplate = sanitizePathSegment(template);

  const baseName = safeTemplate
    .replace(/\{name\}/g, () => safeName)
    .replace(/\{date\}/g, () => stamp.date)
    .replace(/\{time\}/g, () => stamp.time);

  return `${baseName || safeName}${ext}`;
}

const AUDIO_FORMAT_EXTENSIONS: Record<string, string> = {
  Mp3: "mp3",
  Aac: "m4a",
  Flac: "flac",
  Opus: "ogg",
  Wav: "wav",
};

export function getAudioExtension(format: string): string {
  return AUDIO_FORMAT_EXTENSIONS[format] || "mp3";
}

const AUDIO_INPUT_TO_OUTPUT_EXT: Record<string, string> = {
  ".mp3": "mp3",
  ".aac": "m4a",
  ".m4a": "m4a",
  ".flac": "flac",
  ".ogg": "ogg",
  ".opus": "ogg",
  ".wav": "wav",
  ".pcm": "wav",
};

export function resolveAudioCompressionExtension(
  format: AudioCompressionFormat,
  inputPath: string,
): string {
  if (format === "Original") {
    const ext = inputPath.slice(inputPath.lastIndexOf(".")).toLowerCase();
    return AUDIO_INPUT_TO_OUTPUT_EXT[ext] || "mp3";
  }
  return AUDIO_FORMAT_EXTENSIONS[format] || "mp3";
}

export function isValidMediaFile(filePath: string): boolean {
  return getMediaType(filePath) !== null;
}

export function getParentDir(filePath: string): string {
  const idx = lastSeparatorIndex(filePath);
  return idx < 0 ? "" : filePath.slice(0, idx);
}

export function buildOutputPath(
  inputPath: string,
  outputDir: string,
  format?: string,
  template?: string,
  stamp?: { date: string; time: string },
): string {
  const outputName = getOutputFileName(inputPath, format, template, stamp);
  return `${outputDir}${joinSeparator(outputDir)}${outputName}`;
}

/** Output directory for `filePath` under the given output settings. */
export function resolveOutputDir(
  filePath: string,
  settings: { outputMode: OutputMode; outputDir: string | null; subfolderName: string },
): string {
  const parentDir = getParentDir(filePath);
  switch (settings.outputMode) {
    case "subfolder": {
      // The subfolder is a single directory name, never a relative path.
      const safeSubfolder = sanitizePathSegment(settings.subfolderName);
      return `${parentDir}${joinSeparator(parentDir)}${safeSubfolder}`;
    }
    case "customDir":
      return settings.outputDir || parentDir;
    default:
      return parentDir;
  }
}

/** Extensions that can be re-encoded in place when the image format is "Original".
 * `undefined` keeps the input extension; formats with no encoder become PNG. */
const KEEP_IMAGE_EXTENSIONS = new Set([".jpg", ".jpeg", ".png", ".webp", ".avif", ".gif"]);

export function resolveImageOutputFormat(format: string, inputPath: string): string | undefined {
  const ext = inputPath.slice(inputPath.lastIndexOf(".")).toLowerCase();
  // GIF inputs stay GIF regardless of the chosen format (animation is preserved;
  // the output name is forced to .gif, so the bytes must match).
  if (ext === ".gif") return "gif";
  if (format !== "Original") return format.toLowerCase();
  return KEEP_IMAGE_EXTENSIONS.has(ext) ? undefined : "png";
}

/** Convert resolved file paths into QueuedFile objects, filtering unsupported types. */
export function pathsToQueuedFiles(paths: string[]): QueuedFile[] {
  return paths
    .map((path) => {
      const mediaType = getMediaType(path);
      if (!mediaType) return null;
      return {
        id: crypto.randomUUID(),
        path,
        name: getFileName(path),
        size: 0,
        mediaType,
        status: "queued" as const,
        progress: 0,
      };
    })
    .filter((f): f is NonNullable<typeof f> => f !== null);
}
