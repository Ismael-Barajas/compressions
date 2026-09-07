import type { AudioCompressionFormat, MediaType, OutputMode, QueuedFile } from "../types/compression";
import { mediaTypeForExtension } from "./mediaTypes";

export function getMediaType(filePath: string): MediaType | null {
  const dot = filePath.lastIndexOf(".");
  if (dot < 0) return null;
  return mediaTypeForExtension(filePath.slice(dot));
}

export function getFileName(filePath: string): string {
  const sep = filePath.includes("\\") ? "\\" : "/";
  return filePath.split(sep).pop() || filePath;
}

export function formatFileSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));
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
  const date = now.toISOString().slice(0, 10); // YYYY-MM-DD
  const time = `${String(now.getHours()).padStart(2, "0")}-${String(now.getMinutes()).padStart(2, "0")}-${String(now.getSeconds()).padStart(2, "0")}`;
  return { date, time };
}

export function getOutputFileName(
  inputPath: string,
  format?: string,
  template: string = "{name}_compressed",
  stamp: { date: string; time: string } = templateStamp(),
): string {
  const sep = inputPath.includes("\\") ? "\\" : "/";
  const parts = inputPath.split(sep);
  const fileName = parts.pop() || "";
  const dotIndex = fileName.lastIndexOf(".");
  const name = dotIndex >= 0 ? fileName.slice(0, dotIndex) : fileName;
  const inputExt = dotIndex >= 0 ? fileName.slice(dotIndex).toLowerCase() : "";
  // GIF inputs always output as GIF to preserve animation
  const ext = inputExt === ".gif"
    ? ".gif"
    : format
      ? `.${format.toLowerCase()}`
      : inputExt;

  // Sanitize name to prevent path traversal via crafted filenames
  const safeName = name.replace(/[/\\]/g, "_").replace(/\.\./g, "_");

  const baseName = template
    .replace(/\{name\}/g, safeName)
    .replace(/\{date\}/g, stamp.date)
    .replace(/\{time\}/g, stamp.time);

  return `${baseName || name}${ext}`;
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
  const sep = filePath.includes("\\") ? "\\" : "/";
  const parts = filePath.split(sep);
  parts.pop();
  return parts.join(sep);
}

export function buildOutputPath(
  inputPath: string,
  outputDir: string,
  format?: string,
  template?: string,
  stamp?: { date: string; time: string },
): string {
  const sep = outputDir.includes("\\") ? "\\" : "/";
  const outputName = getOutputFileName(inputPath, format, template, stamp);
  return `${outputDir}${sep}${outputName}`;
}

/** Output directory for `filePath` under the given output settings. */
export function resolveOutputDir(
  filePath: string,
  settings: { outputMode: OutputMode; outputDir: string | null; subfolderName: string },
): string {
  const parentDir = getParentDir(filePath);
  switch (settings.outputMode) {
    case "subfolder": {
      const sep = parentDir.includes("\\") ? "\\" : "/";
      return `${parentDir}${sep}${settings.subfolderName}`;
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
  if (format !== "Original") return format.toLowerCase();
  const ext = inputPath.slice(inputPath.lastIndexOf(".")).toLowerCase();
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
