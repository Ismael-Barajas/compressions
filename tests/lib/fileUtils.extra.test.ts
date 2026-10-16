import { describe, it, expect } from "vitest";
import {
  buildOutputPath,
  formatFileSize,
  getFileName,
  getOutputFileName,
  getParentDir,
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
    expect(stamp.date).toBe("2026-01-02");
  });

  it("uses the local date, not UTC, near midnight", () => {
    // Local 00:30 on Jan 1: in any zone east of UTC toISOString() still says Dec 31.
    const d = new Date(2026, 0, 1, 0, 30, 0);
    const stamp = templateStamp(d);
    expect(stamp.date).toBe(`${d.getFullYear()}-01-01`);
    expect(stamp.time).toBe("00-30-00");
  });
});

describe("output name sanitization", () => {
  const stamp = { date: "2026-01-02", time: "03-04-05" };

  it("neutralizes separators and .. in the template", () => {
    expect(getOutputFileName("/in/a.mp4", undefined, "../{name}", stamp)).toBe("__a.mp4");
    expect(getOutputFileName("/in/a.mp4", undefined, "..\\..\\{name}", stamp)).toBe("____a.mp4");
  });

  it("inserts a file name containing $& literally", () => {
    expect(getOutputFileName("/in/a$&b.mp4", undefined, "{name}_x", stamp)).toBe("a$&b_x.mp4");
    expect(getOutputFileName("/in/a$1b.mp4", undefined, "{name}", stamp)).toBe("a$1b.mp4");
  });

  it("falls back to the sanitized stem for an empty template", () => {
    expect(getOutputFileName("/in/..x.mp4", undefined, "", stamp)).toBe("_x.mp4");
  });

  it("sanitizes the subfolder name", () => {
    const dir = resolveOutputDir("C:\\vids\\c.mp4", {
      outputMode: "subfolder",
      outputDir: null,
      subfolderName: "..\\..",
    });
    expect(dir.startsWith("C:\\vids\\")).toBe(true);
    expect(dir.slice("C:\\vids\\".length)).not.toMatch(/[/\\]|\.\./);
    expect(
      resolveOutputDir("/a/b/c.mp4", { outputMode: "subfolder", outputDir: null, subfolderName: "../up" }),
    ).toBe("/a/b/__up");
  });
});

describe("path helpers with mixed separators", () => {
  it("getFileName / getParentDir use the last separator of either kind", () => {
    expect(getFileName("C:/Users\\me/clip.mp4")).toBe("clip.mp4");
    expect(getFileName("C:\\Users/me\\clip.mp4")).toBe("clip.mp4");
    expect(getParentDir("C:/Users\\me/clip.mp4")).toBe("C:/Users\\me");
    expect(getParentDir("clip.mp4")).toBe("");
  });

  it("buildOutputPath joins with the separator used last in the dir", () => {
    expect(buildOutputPath("C:/in\\a.mp4", "C:/out\\sub")).toBe("C:/out\\sub\\a_compressed.mp4");
  });
});

describe("formatFileSize edge cases", () => {
  it("clamps to the largest unit and handles bad input", () => {
    expect(formatFileSize(2 ** 50)).toBe("1.0 PB");
    expect(formatFileSize(2 ** 60)).toBe("1024.0 PB");
    expect(formatFileSize(-5)).toBe("0 B");
    expect(formatFileSize(NaN)).toBe("0 B");
    expect(formatFileSize(Infinity)).toBe("0 B");
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

  it("keeps GIF inputs as GIF whatever format is selected", () => {
    expect(resolveImageOutputFormat("WebP", "/anim.gif")).toBe("gif");
    expect(resolveImageOutputFormat("Original", "/anim.GIF")).toBe("gif");
    expect(getOutputFileName("/anim.gif", resolveImageOutputFormat("WebP", "/anim.gif"))).toBe(
      "anim_compressed.gif",
    );
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
