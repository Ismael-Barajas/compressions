// Regression tests for bugs found in the October 2026 end-to-end run against the
// real app (wrong-format outputs, WebM container, unsafe names, re-added folders).
import { describe, it, expect, vi, beforeEach } from "vitest";

Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: (query: string) => ({
    matches: false, media: query, onchange: null,
    addListener: () => {}, removeListener: () => {},
    addEventListener: () => {}, removeEventListener: () => {}, dispatchEvent: () => false,
  }),
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));
const scanPaths = vi.fn<(paths: string[]) => Promise<string[]>>();
vi.mock("../../src/lib/commands", () => ({
  scanPaths,
  clearThumbnailCache: vi.fn(() => Promise.resolve()),
  getSupportedMedia: vi.fn(() => Promise.reject(new Error("no tauri"))),
}));

const {
  getOutputFileName,
  isSameFormat,
  keptOriginal,
  resolveOutputDir,
  resolveVideoOutputFormat,
} = await import("../../src/lib/fileUtils");
const { scanForQueue } = await import("../../src/lib/scanForQueue");
const { useCompressionStore } = await import("../../src/stores/compressionStore");

import type { CompressionResult, VideoOptions } from "../../src/types/compression";

const video = (o: Partial<VideoOptions>): VideoOptions => ({
  codec: "H264",
  crf: 23,
  resolution: null,
  bitrate: null,
  framerate: null,
  audioCodec: "AAC",
  audioBitrate: "128k",
  ...o,
});

describe("resolveVideoOutputFormat", () => {
  it("writes MP4 when the source container cannot hold the codecs", () => {
    expect(resolveVideoOutputFormat(video({}), "C:\\v\\clip.webm")).toBe("mp4");
    expect(resolveVideoOutputFormat(video({ codec: "H265" }), "a.WEBM")).toBe("mp4");
    expect(resolveVideoOutputFormat(video({ codec: "AV1" }), "a.mov")).toBe("mp4");
    expect(resolveVideoOutputFormat(video({ audioCodec: "Opus" }), "a.avi")).toBe("mp4");
    expect(resolveVideoOutputFormat(video({ codec: "H265" }), "a.wmv")).toBe("mp4");
  });

  it("keeps the source container when it fits", () => {
    expect(resolveVideoOutputFormat(video({ codec: "AV1", audioCodec: "Opus" }), "a.webm")).toBeUndefined();
    expect(resolveVideoOutputFormat(video({}), "a.mov")).toBeUndefined();
    expect(resolveVideoOutputFormat(video({ codec: "AV1", audioCodec: "Opus" }), "a.mkv")).toBeUndefined();
    expect(resolveVideoOutputFormat(video({ codec: "H265" }), "a.mp4")).toBeUndefined();
  });
});

describe("kept-original detection", () => {
  const result = (inputPath: string, outputPath: string, i: number, o: number): CompressionResult => ({
    jobId: "j", inputPath, outputPath, inputSize: i, outputSize: o, durationMs: 1, success: true, error: null,
  });

  it("treats jpg/jpeg as one format", () => {
    expect(isSameFormat("a.JPG", "a_c.jpeg")).toBe(true);
    expect(isSameFormat("a.png", "a_c.jpeg")).toBe(false);
  });

  it("only same-format, not-smaller outputs are kept originals", () => {
    expect(keptOriginal(result("a.png", "a_c.png", 10, 10))).toBe(true);
    expect(keptOriginal(result("a.png", "a_c.jpeg", 10, 12))).toBe(false);
    expect(keptOriginal(result("a.png", "a_c.png", 10, 4))).toBe(false);
  });
});

describe("output names and folders", () => {
  it("replaces characters Windows forbids in the template", () => {
    expect(getOutputFileName("C:\\x\\pic.bmp", "jpeg", "{name}:x")).toBe("pic_x.jpeg");
    expect(getOutputFileName("C:\\x\\pic.bmp", "jpeg", '{name}?*<>|"')).toBe("pic______.jpeg");
  });

  it("an empty subfolder name falls back to 'compressed'", () => {
    const settings = { outputMode: "subfolder" as const, outputDir: null, subfolderName: "  " };
    expect(resolveOutputDir("C:\\x\\pic.bmp", settings)).toBe("C:\\x\\compressed");
  });
});

describe("scanForQueue", () => {
  beforeEach(() => {
    useCompressionStore.getState().setSubfolderName("compressed");
    useCompressionStore.getState().setOutputTemplate("{name}_compressed");
  });

  it("skips earlier outputs found inside a scanned folder", async () => {
    scanPaths.mockResolvedValueOnce([
      "C:\\d\\a.jpg",
      "C:\\d\\a_compressed.jpeg",
      "C:\\d\\a_compressed_2.jpeg",
      "C:\\d\\compressed\\b_c.png",
      "C:\\d\\sub\\c.mp4",
    ]);
    expect(await scanForQueue(["C:\\d"])).toEqual(["C:\\d\\a.jpg", "C:\\d\\sub\\c.mp4"]);
  });

  it("keeps a file the user picked directly, even if it looks like output", async () => {
    scanPaths.mockResolvedValueOnce(["C:\\d\\a_compressed.jpeg"]);
    expect(await scanForQueue(["C:\\d\\a_compressed.jpeg"])).toEqual(["C:\\d\\a_compressed.jpeg"]);
  });
});
