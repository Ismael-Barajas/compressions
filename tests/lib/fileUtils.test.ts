import { describe, it, expect } from "vitest";
import {
  getMediaType,
  getFileName,
  formatFileSize,
  getSavingsPercent,
  getOutputFileName,
  getAudioExtension,
  isValidMediaFile,
  getParentDir,
  buildOutputPath,
} from "../../src/lib/fileUtils";

describe("getMediaType", () => {
  it("detects video extensions", () => {
    for (const ext of ["mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "ts"]) {
      expect(getMediaType(`file.${ext}`)).toBe("video");
    }
  });

  it("detects image extensions", () => {
    for (const ext of ["jpg", "jpeg", "png", "webp", "avif", "bmp", "tiff", "tif", "gif"]) {
      expect(getMediaType(`file.${ext}`)).toBe("image");
    }
  });

  it("detects pdf", () => {
    expect(getMediaType("doc.pdf")).toBe("pdf");
  });

  it("is case insensitive", () => {
    expect(getMediaType("file.MP4")).toBe("video");
    expect(getMediaType("file.PNG")).toBe("image");
    expect(getMediaType("file.PDF")).toBe("pdf");
  });

  it("returns null for unknown", () => {
    expect(getMediaType("file.xyz")).toBeNull();
    expect(getMediaType("noext")).toBeNull();
  });
});

describe("getFileName", () => {
  it("extracts from Unix path", () => {
    expect(getFileName("/home/user/video.mp4")).toBe("video.mp4");
  });

  it("extracts from Windows path", () => {
    expect(getFileName("C:\\Users\\user\\video.mp4")).toBe("video.mp4");
  });

  it("returns input if no separator", () => {
    expect(getFileName("video.mp4")).toBe("video.mp4");
  });
});

describe("formatFileSize", () => {
  it("formats 0 bytes", () => {
    expect(formatFileSize(0)).toBe("0 B");
  });

  it("formats bytes", () => {
    expect(formatFileSize(500)).toBe("500 B");
  });

  it("formats KB", () => {
    expect(formatFileSize(1536)).toBe("1.5 KB");
  });

  it("formats MB", () => {
    expect(formatFileSize(1024 * 1024)).toBe("1.0 MB");
  });

  it("formats GB", () => {
    expect(formatFileSize(1024 * 1024 * 1024 * 2.5)).toBe("2.5 GB");
  });
});

describe("getSavingsPercent", () => {
  it("calculates savings", () => {
    expect(getSavingsPercent(100, 70)).toBe(30);
  });

  it("handles zero input (div by zero)", () => {
    expect(getSavingsPercent(0, 50)).toBe(0);
  });

  it("handles same size", () => {
    expect(getSavingsPercent(100, 100)).toBe(0);
  });

  it("handles larger output (negative savings)", () => {
    expect(getSavingsPercent(100, 150)).toBe(-50);
  });
});

describe("getOutputFileName", () => {
  it("applies default template", () => {
    const result = getOutputFileName("/path/video.mp4");
    expect(result).toMatch(/^video_compressed\.mp4$/);
  });

  it("substitutes {name}", () => {
    const result = getOutputFileName("/path/video.mp4", undefined, "{name}_small");
    expect(result).toBe("video_small.mp4");
  });

  it("substitutes {date}", () => {
    const result = getOutputFileName("/path/video.mp4", undefined, "{name}_{date}");
    expect(result).toMatch(/^video_\d{4}-\d{2}-\d{2}\.mp4$/);
  });

  it("substitutes {time}", () => {
    const result = getOutputFileName("/path/video.mp4", undefined, "{name}_{time}");
    expect(result).toMatch(/^video_\d{2}-\d{2}-\d{2}\.mp4$/);
  });

  it("changes format extension", () => {
    const result = getOutputFileName("/path/image.png", "webp");
    expect(result).toBe("image_compressed.webp");
  });

  it("preserves GIF extension regardless of format", () => {
    const result = getOutputFileName("/path/anim.gif", "webp");
    expect(result).toBe("anim_compressed.gif");
  });

  it("handles Windows paths", () => {
    const result = getOutputFileName("C:\\Users\\file.mp4");
    expect(result).toBe("file_compressed.mp4");
  });
});

describe("getAudioExtension", () => {
  it("maps known formats", () => {
    expect(getAudioExtension("Mp3")).toBe("mp3");
    expect(getAudioExtension("Aac")).toBe("m4a");
    expect(getAudioExtension("Flac")).toBe("flac");
    expect(getAudioExtension("Opus")).toBe("ogg");
    expect(getAudioExtension("Wav")).toBe("wav");
  });

  it("defaults to mp3 for unknown", () => {
    expect(getAudioExtension("Unknown")).toBe("mp3");
  });
});

describe("isValidMediaFile", () => {
  it("returns true for valid files", () => {
    expect(isValidMediaFile("video.mp4")).toBe(true);
    expect(isValidMediaFile("image.png")).toBe(true);
    expect(isValidMediaFile("doc.pdf")).toBe(true);
  });

  it("returns false for invalid files", () => {
    expect(isValidMediaFile("file.txt")).toBe(false);
  });
});

describe("getParentDir", () => {
  it("extracts Unix parent", () => {
    expect(getParentDir("/home/user/video.mp4")).toBe("/home/user");
  });

  it("extracts Windows parent", () => {
    expect(getParentDir("C:\\Users\\user\\video.mp4")).toBe("C:\\Users\\user");
  });
});

describe("buildOutputPath", () => {
  it("combines dir and filename", () => {
    const result = buildOutputPath("/home/video.mp4", "/output");
    expect(result).toBe("/output/video_compressed.mp4");
  });

  it("uses Windows separator when dir is Windows", () => {
    const result = buildOutputPath("C:\\in\\video.mp4", "C:\\out");
    expect(result).toBe("C:\\out\\video_compressed.mp4");
  });

  it("applies format and template", () => {
    const result = buildOutputPath("/in/image.png", "/out", "webp", "{name}_opt");
    expect(result).toBe("/out/image_opt.webp");
  });
});
