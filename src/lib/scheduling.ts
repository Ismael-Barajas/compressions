import type { MediaType, QueuedFile } from "../types/compression";

export function sortQueuedFilesForCompression(
  files: QueuedFile[],
  mediaType: MediaType,
): QueuedFile[] {
  const direction = mediaType === "image" ? -1 : 1;
  return files
    .filter((file) => file.mediaType === mediaType)
    .sort((a, b) => (a.size - b.size) * direction);
}
