import { bench, describe } from "vitest";
import { getOutputFileName, getMediaType } from "../../src/lib/fileUtils";

describe("fileUtils benchmarks", () => {
  bench("getOutputFileName x10,000", () => {
    for (let i = 0; i < 10_000; i++) {
      getOutputFileName("/path/to/video.mp4", undefined, "{name}_{date}_{time}");
    }
  });

  bench("getMediaType x100,000", () => {
    const extensions = ["file.mp4", "file.png", "file.pdf", "file.webp", "file.mkv"];
    for (let i = 0; i < 100_000; i++) {
      getMediaType(extensions[i % extensions.length]);
    }
  });
});
