import type { MediaType } from "../types/compression";

const VIDEO_EXTENSIONS = new Set([
  ".mp4", ".mkv", ".avi", ".mov", ".webm", ".flv", ".wmv", ".m4v", ".ts",
]);

const IMAGE_EXTENSIONS = new Set([
  ".jpg", ".jpeg", ".png", ".webp", ".avif", ".bmp", ".tiff", ".tif", ".gif",
]);

export function getMediaType(filePath: string): MediaType | null {
  const ext = filePath.slice(filePath.lastIndexOf(".")).toLowerCase();
  if (VIDEO_EXTENSIONS.has(ext)) return "video";
  if (IMAGE_EXTENSIONS.has(ext)) return "image";
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
): string {
  const sep = inputPath.includes("\\") ? "\\" : "/";
  const parts = inputPath.split(sep);
  const fileName = parts.pop() || "";
  const dotIndex = fileName.lastIndexOf(".");
  const name = dotIndex >= 0 ? fileName.slice(0, dotIndex) : fileName;
  const ext = format
    ? `.${format.toLowerCase()}`
    : dotIndex >= 0
      ? fileName.slice(dotIndex)
      : "";
  return `${name}_compressed${ext}`;
}

export function isValidMediaFile(filePath: string): boolean {
  return getMediaType(filePath) !== null;
}
