import { describe, expect, it } from "vitest";
import type { QueuedFile } from "../../src/types/compression";
import { sortQueuedFilesForCompression } from "../../src/lib/scheduling";

function file(id: string, mediaType: QueuedFile["mediaType"], size: number): QueuedFile {
  return {
    id,
    path: `${id}.bin`,
    name: `${id}.bin`,
    size,
    mediaType,
    status: "queued",
    progress: 0,
  };
}

describe("sortQueuedFilesForCompression", () => {
  it("prioritizes short sequential jobs and balances parallel image workers", () => {
    const files = [
      file("image-small", "image", 10),
      file("image-large", "image", 100),
      file("video-large", "video", 1000),
      file("video-small", "video", 20),
      file("pdf-large", "pdf", 500),
      file("pdf-small", "pdf", 5),
      file("audio-large", "audio", 300),
      file("audio-small", "audio", 30),
    ];

    expect(sortQueuedFilesForCompression(files, "image").map((f) => f.id)).toEqual([
      "image-large",
      "image-small",
    ]);
    expect(sortQueuedFilesForCompression(files, "video").map((f) => f.id)).toEqual([
      "video-small",
      "video-large",
    ]);
    expect(sortQueuedFilesForCompression(files, "pdf").map((f) => f.id)).toEqual([
      "pdf-small",
      "pdf-large",
    ]);
    expect(sortQueuedFilesForCompression(files, "audio").map((f) => f.id)).toEqual([
      "audio-small",
      "audio-large",
    ]);
  });
});
