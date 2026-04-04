import type { MediaType } from "../types/compression";

const VIDEO_EXTENSIONS = new Set([
  ".mp4", ".mkv", ".avi", ".mov", ".webm", ".flv", ".wmv", ".m4v", ".ts",
]);

const IMAGE_EXTENSIONS = new Set([
  ".jpg", ".jpeg", ".png", ".webp", ".avif", ".bmp", ".tiff", ".tif", ".gif",
]);

const PDF_EXTENSIONS = new Set([".pdf"]);

export function getMediaType(filePath: string): MediaType | null {
  const ext = filePath.slice(filePath.lastIndexOf(".")).toLowerCase();
  if (VIDEO_EXTENSIONS.has(ext)) return "video";
  if (IMAGE_EXTENSIONS.has(ext)) return "image";
  if (PDF_EXTENSIONS.has(ext)) return "pdf";
  return null;
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

export function getOutputFileName(
  inputPath: string,
  format?: string,
  template: string = "{name}_compressed",
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

  const now = new Date();
  const date = now.toISOString().slice(0, 10); // YYYY-MM-DD
  const time = `${String(now.getHours()).padStart(2, "0")}-${String(now.getMinutes()).padStart(2, "0")}-${String(now.getSeconds()).padStart(2, "0")}`;

  const baseName = template
    .replace(/\{name\}/g, name)
    .replace(/\{date\}/g, date)
    .replace(/\{time\}/g, time);

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
): string {
  const sep = outputDir.includes("\\") ? "\\" : "/";
  const outputName = getOutputFileName(inputPath, format, template);
  return `${outputDir}${sep}${outputName}`;
}
