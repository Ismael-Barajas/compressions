import { describe, it, expect } from "vitest";
import {
  buildOutputPath,
  getOutputFileName,
  resolveImageOutputFormat,
  resolveOutputDir,
  templateStamp,
  getMediaType,
} from "../../src/lib/fileUtils";
import {
  _setSupportedMediaForTests,
  dialogFilters,
  mediaTypeForExtension,
} from "../../src/lib/mediaTypes";

describe("templateStamp", () => {
  it("is applied consistently when passed explicitly", () => {
    const stamp = { date: "2026-01-02", time: "03-04-05" };
    expect(getOutputFileName("/a/b.mp4", undefined, "{name}_{date}_{time}", stamp)).toBe(
      "b_2026-01-02_03-04-05.mp4",
    );
    expect(buildOutputPath("/a/b.mp4", "/out", "webm", "{time}", stamp)).toBe("/out/03-04-05.webm");
  });

  it("formats from a Date", () => {
    const d = new Date(2026, 0, 2, 3, 4, 5);
    const stamp = templateStamp(d);
    expect(stamp.time).toBe("03-04-05");
    expect(stamp.date).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});

describe("resolveOutputDir", () => {
  it("handles each output mode", () => {
    expect(resolveOutputDir("/a/b/c.mp4", { outputMode: "sameDir", outputDir: null, subfolderName: "x" })).toBe("/a/b");
    expect(resolveOutputDir("/a/b/c.mp4", { outputMode: "subfolder", outputDir: null, subfolderName: "compressed" })).toBe("/a/b/compressed");
    expect(resolveOutputDir("/a/b/c.mp4", { outputMode: "customDir", outputDir: "/out", subfolderName: "x" })).toBe("/out");
    expect(resolveOutputDir("/a/b/c.mp4", { outputMode: "customDir", outputDir: null, subfolderName: "x" })).toBe("/a/b");
  });

  it("uses backslashes for Windows subfolders", () => {
    expect(
      resolveOutputDir("C:\\vids\\c.mp4", { outputMode: "subfolder", outputDir: null, subfolderName: "sub" }),
    ).toBe("C:\\vids\\sub");
  });
});

describe("resolveImageOutputFormat", () => {
  it("keeps re-encodable formats and falls back to png otherwise", () => {
    expect(resolveImageOutputFormat("Original", "/a.JPG")).toBeUndefined();
    expect(resolveImageOutputFormat("Original", "/a.webp")).toBeUndefined();
    expect(resolveImageOutputFormat("Original", "/a.bmp")).toBe("png");
    expect(resolveImageOutputFormat("Original", "/a.heic")).toBe("png");
    expect(resolveImageOutputFormat("WebP", "/a.bmp")).toBe("webp");
  });
});

describe("mediaTypes", () => {
  it("classifies from the active list and builds dialog filters", () => {
    expect(mediaTypeForExtension(".MP3")).toBe("audio");
    expect(mediaTypeForExtension(".xyz")).toBeNull();
    const filters = dialogFilters();
    expect(filters[0].name).toBe("Media Files");
    expect(filters[0].extensions).toContain("flac");
    expect(filters[0].extensions).toContain("pdf");
    expect(filters.find((f) => f.name === "Audio Files")?.extensions).toContain("opus");
  });

  it("switches to a backend-provided list", () => {
    _setSupportedMediaForTests({ video: ["zzz"], image: [], audio: [], pdf: [] });
    expect(getMediaType("clip.zzz")).toBe("video");
    expect(getMediaType("clip.mp4")).toBeNull();
    _setSupportedMediaForTests(null);
    expect(getMediaType("clip.mp4")).toBe("video");
  });
});
