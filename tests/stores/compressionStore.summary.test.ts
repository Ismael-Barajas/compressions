/**
 * @vitest-environment jsdom
 */
import { describe, it, expect, beforeEach, vi } from "vitest";

const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, value: string) => { store[key] = value; },
    removeItem: (key: string) => { delete store[key]; },
    clear: () => { store = {}; },
    get length() { return Object.keys(store).length; },
    key: (i: number) => Object.keys(store)[i] ?? null,
  };
})();
Object.defineProperty(globalThis, "localStorage", { value: localStorageMock, writable: true });
Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: false, media: query, onchange: null,
    addListener: vi.fn(), removeListener: vi.fn(),
    addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn(),
  })),
});

vi.mock("../../src/lib/commands", () => ({
  clearThumbnailCache: vi.fn(() => Promise.resolve()),
}));

const { useCompressionStore, deriveSummary, getStoredFileView } = await import("../../src/stores/compressionStore");

import type { QueuedFile, ProgressPayload, CompressionResult } from "../../src/types/compression";

let counter = 0;
function makeFile(overrides: Partial<QueuedFile> = {}): QueuedFile {
  const id = `f${++counter}`;
  return {
    id,
    path: `/path/${id}.mp4`,
    name: `${id}.mp4`,
    size: 1024,
    mediaType: "video",
    status: "queued",
    progress: 0,
    ...overrides,
  };
}

function progress(jobId: string, percent: number): ProgressPayload {
  return { jobId, fileName: "x", percent, currentFrame: null, totalFrames: null, speed: null, etaSeconds: null };
}

const store = () => useCompressionStore.getState();

function result(
  jobId: string,
  inputSize: number,
  outputSize: number,
  inputPath = "/in.mp4",
  outputPath = "/out.mp4",
): CompressionResult {
  return {
    jobId, inputPath, outputPath, inputSize, outputSize,
    durationMs: 1, success: true, error: null,
  };
}

function expectConsistent() {
  expect(store().summary).toEqual(deriveSummary(store().files));
}

describe("summary counters", () => {
  beforeEach(() => {
    useCompressionStore.setState({ files: [], summary: deriveSummary([]), filesRevision: 0 });
  });

  it("tracks media and status counts through add/remove", () => {
    const v = makeFile();
    const i = makeFile({ mediaType: "image", path: "/a.png" });
    const p = makeFile({ mediaType: "pdf", path: "/a.pdf" });
    store().addFiles([v, i, p]);
    expect(store().summary).toMatchObject({ total: 3, queued: 3, video: 1, image: 1, pdf: 1, audio: 0, queuedVideos: 1 });
    expect(store().filesRevision).toBe(1);

    store().removeFile(v.id);
    expect(store().summary).toMatchObject({ total: 2, video: 0, queuedVideos: 0 });
    expect(store().filesRevision).toBe(2);
  });

  it("follows status transitions and progress sums", () => {
    const a = makeFile();
    const b = makeFile();
    store().addFiles([a, b]);

    store().setFileStatus(a.id, "processing", "job-a");
    expect(store().summary).toMatchObject({ queued: 1, processing: 1, queuedVideos: 1 });

    store().updateProgress("job-a", progress("job-a", 40));
    expect(store().summary.progressSum).toBe(40);
    store().updateProgress("job-a", progress("job-a", 70));
    expect(store().summary.progressSum).toBe(70);

    store().setFileStatus(b.id, "processing", "job-b");
    store().updateProgress("job-b", progress("job-b", 10));
    expect(store().summary.progressSum).toBe(80);

    store().markComplete("job-a", {
      jobId: "job-a", inputPath: a.path, outputPath: "/o", inputSize: 10, outputSize: 5,
      durationMs: 1, success: true, error: null,
    });
    expect(store().summary).toMatchObject({ processing: 1, complete: 1, progressSum: 10 });

    store().markError("job-b", "boom");
    expect(store().summary).toMatchObject({ processing: 0, complete: 1, error: 1, progressSum: 0 });

    store().retryFile(b.id);
    expect(store().summary).toMatchObject({ queued: 1, error: 0, queuedVideos: 1 });
    expect(store().summary).toEqual(deriveSummary(store().files));
  });

  it("cancelAllCompression resets queued/processing and recomputes", () => {
    const a = makeFile();
    const b = makeFile();
    store().addFiles([a, b]);
    store().setFileStatus(a.id, "processing", "job-a");
    store().updateProgress("job-a", progress("job-a", 50));
    store().cancelAllCompression();
    expect(store()._cancelRequested).toBe(true);
    expect(store().summary).toMatchObject({ queued: 2, processing: 0, progressSum: 0 });
    expect(store().files.every((f) => f.jobId === undefined && f.progress === 0)).toBe(true);
  });
});

describe("per-file updates", () => {
  beforeEach(() => {
    useCompressionStore.setState({ files: [], summary: deriveSummary([]) });
  });

  it("only replaces the touched file object", () => {
    const files = [makeFile(), makeFile(), makeFile()];
    store().addFiles(files);
    store().setFileStatus(files[1].id, "processing", "job-1");
    const before = store().files;
    store().updateProgress("job-1", progress("job-1", 33));
    const after = store().files;
    expect(after).not.toBe(before);
    expect(after[0]).toBe(before[0]);
    expect(after[2]).toBe(before[2]);
    expect(after[1]).not.toBe(before[1]);
    expect(after[1].progress).toBe(33);
  });

  it("skips a no-op progress update", () => {
    const f = makeFile();
    store().addFiles([f]);
    store().setFileStatus(f.id, "processing", "job-1");
    store().updateProgress("job-1", progress("job-1", 20));
    const before = store().files;
    store().updateProgress("job-1", progress("job-1", 20));
    expect(store().files).toBe(before);
  });

  it("resolves jobIds after files were replaced wholesale", () => {
    const f = makeFile({ status: "processing", jobId: "job-x" });
    useCompressionStore.setState({ files: [f], summary: deriveSummary([f]) });
    store().updateProgress("job-x", progress("job-x", 12));
    expect(store().files[0].progress).toBe(12);
    store().markError("job-x", "nope");
    expect(store().files[0].status).toBe("error");
  });

  it("ignores unknown ids and jobIds", () => {
    const f = makeFile();
    store().addFiles([f]);
    const before = store().files;
    store().updateProgress("missing", progress("missing", 1));
    store().markComplete("missing", {
      jobId: "missing", inputPath: "", outputPath: "", inputSize: 0, outputSize: 0,
      durationMs: 0, success: true, error: null,
    });
    store().setThumbnailPath("nope", "/t.jpg");
    expect(store().files).toBe(before);
  });

  it("markErrorByIds updates many files in one pass with a status filter", () => {
    const a = makeFile();
    const b = makeFile();
    const c = makeFile();
    store().addFiles([a, b, c]);
    store().setFileStatus(c.id, "processing", "job-c");
    store().markComplete("job-c", {
      jobId: "job-c", inputPath: c.path, outputPath: "/o", inputSize: 1, outputSize: 1,
      durationMs: 0, success: true, error: null,
    });

    store().markErrorByIds([a.id, b.id, c.id], "batch failed", ["queued", "processing"]);
    const byId = new Map(store().files.map((f) => [f.id, f]));
    expect(byId.get(a.id)?.status).toBe("error");
    expect(byId.get(b.id)?.error).toBe("batch failed");
    expect(byId.get(c.id)?.status).toBe("complete");
    expect(store().summary).toMatchObject({ error: 2, complete: 1, queued: 0 });
  });
});

describe("lane summaries", () => {
  beforeEach(() => {
    useCompressionStore.setState({ files: [], summary: deriveSummary([]), filesRevision: 0 });
  });

  it("tracks per-lane counts through the file lifecycle", () => {
    const v1 = makeFile();
    const v2 = makeFile();
    const img = makeFile({ mediaType: "image", path: "/lane.png" });
    const pdf = makeFile({ mediaType: "pdf", path: "/lane.pdf" });
    const aud = makeFile({ mediaType: "audio", path: "/lane.mp3" });
    store().addFiles([v1, v2, img, pdf, aud]);
    expectConsistent();
    expect(store().summary.lanes.video).toEqual({ queued: 2, processing: 0, complete: 0, error: 0, progressSum: 0 });
    expect(store().summary.lanes.image.queued).toBe(1);
    expect(store().summary.lanes.audio.queued).toBe(1);

    store().setFileStatus(v1.id, "processing", "job-v1");
    store().setFileStatus(img.id, "processing", "job-img");
    expectConsistent();
    expect(store().summary.lanes.video).toMatchObject({ queued: 1, processing: 1 });

    store().updateProgress("job-v1", progress("job-v1", 30));
    store().updateProgress("job-img", progress("job-img", 50));
    expectConsistent();
    expect(store().summary.lanes.video.progressSum).toBe(30);
    expect(store().summary.lanes.image.progressSum).toBe(50);

    store().markComplete("job-v1", result("job-v1", 100, 40));
    expectConsistent();
    expect(store().summary.lanes.video).toMatchObject({ queued: 1, processing: 0, complete: 1, progressSum: 0 });

    store().markError("job-img", "bad pixels");
    expectConsistent();
    expect(store().summary.lanes.image).toMatchObject({ processing: 0, error: 1, progressSum: 0 });

    store().retryFile(img.id);
    expectConsistent();
    expect(store().summary.lanes.image).toMatchObject({ queued: 1, error: 0 });

    store().setFileStatus(pdf.id, "processing", "job-pdf");
    store().updateProgress("job-pdf", progress("job-pdf", 20));
    store().cancelAllCompression();
    expectConsistent();
    expect(store().summary.lanes.pdf).toEqual({ queued: 1, processing: 0, complete: 0, error: 0, progressSum: 0 });
    expect(store().summary.lanes.video.complete).toBe(1);

    store().removeFile(v1.id);
    expectConsistent();
    expect(store().summary.lanes.video).toEqual({ queued: 1, processing: 0, complete: 0, error: 0, progressSum: 0 });
  });

  it("sums saved sizes and counts kept originals", () => {
    const a = makeFile();
    const b = makeFile({ mediaType: "image", path: "/saved.png" });
    store().addFiles([a, b]);
    store().setFileStatus(a.id, "processing", "job-a");
    store().setFileStatus(b.id, "processing", "job-b");

    store().markComplete("job-a", result("job-a", 1000, 400));
    expectConsistent();
    expect(store().summary).toMatchObject({ savedIn: 1000, savedOut: 400, keptOriginal: 0 });

    // Keep-original-if-larger reports outputSize === inputSize.
    store().markComplete("job-b", result("job-b", 500, 500, "/saved.png", "/saved_c.png"));
    expectConsistent();
    expect(store().summary).toMatchObject({ savedIn: 1500, savedOut: 900, keptOriginal: 1 });

    store().retryFile(b.id);
    expectConsistent();
    expect(store().summary).toMatchObject({ savedIn: 1000, savedOut: 400, keptOriginal: 0 });

    store().retryFile(a.id);
    expectConsistent();
    expect(store().summary).toMatchObject({ savedIn: 0, savedOut: 0, keptOriginal: 0 });
  });

  it("does not count tool conversions as savings or larger conversions as kept originals", () => {
    const v = makeFile();
    const img = makeFile({ mediaType: "image", path: "/conv.png" });
    store().addFiles([v, img]);
    store().setFileStatus(v.id, "processing", "job-v");
    store().setFileStatus(img.id, "processing", "job-i");

    // Extract audio: a 1000-byte video becomes a 50-byte MP3. Not a saving.
    store().markComplete("job-v", result("job-v", 1000, 50, "/in.mp4", "/in_compressed.mp3"));
    // PNG → JPEG that came out larger: kept as JPEG, not "kept original".
    store().markComplete("job-i", result("job-i", 100, 120, "/conv.png", "/conv_c.jpeg"));
    expectConsistent();
    expect(store().summary).toMatchObject({ savedIn: 100, savedOut: 120, keptOriginal: 0 });
  });

  it("a progress tick only replaces the touched lane object", () => {
    const v = makeFile();
    const i = makeFile({ mediaType: "image", path: "/tick.png" });
    store().addFiles([v, i]);
    store().setFileStatus(v.id, "processing", "job-v");
    const before = store().summary.lanes;
    store().updateProgress("job-v", progress("job-v", 15));
    const after = store().summary.lanes;
    expect(after.image).toBe(before.image);
    expect(after.pdf).toBe(before.pdf);
    expect(after.audio).toBe(before.audio);
    expect(after.video).not.toBe(before.video);
    expect(after.video.progressSum).toBe(15);
  });

  it("never mutates the shared empty lanes", () => {
    store().clearFiles();
    const f = makeFile();
    store().addFiles([f]);
    store().setFileStatus(f.id, "processing", "job-e");
    store().clearFiles();
    expect(store().summary.lanes.video).toEqual({ queued: 0, processing: 0, complete: 0, error: 0, progressSum: 0 });
    expect(deriveSummary([]).lanes.video.queued).toBe(0);
  });
});

describe("file view preference", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("defaults to list", () => {
    expect(store().fileView).toBe("list");
    expect(getStoredFileView()).toBe("list");
  });

  it("setFileView updates state and persists", () => {
    store().setFileView("pipeline");
    expect(store().fileView).toBe("pipeline");
    expect(localStorage.getItem("compressions-file-view")).toBe("pipeline");
    expect(getStoredFileView()).toBe("pipeline");
    store().setFileView("list");
    expect(localStorage.getItem("compressions-file-view")).toBe("list");
  });

  it("falls back to list for an invalid stored value", () => {
    localStorage.setItem("compressions-file-view", "grid");
    expect(getStoredFileView()).toBe("list");
  });
});
