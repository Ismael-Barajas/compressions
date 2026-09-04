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

// A Channel stand-in that lets tests push backend events.
class FakeChannel<T> {
  onmessage: (event: T) => void = () => {};
}
vi.mock("@tauri-apps/api/core", () => ({
  Channel: FakeChannel,
  invoke: vi.fn(),
  convertFileSrc: (p: string) => p,
}));

const commands = {
  compressVideosBatch: vi.fn(),
  compressImagesBatch: vi.fn(),
  compressPdfsBatch: vi.fn(),
  compressAudioBatch: vi.fn(),
  extractAudio: vi.fn(),
  extractAudioBatch: vi.fn(),
  convertVideoToGif: vi.fn(),
  convertVideosToGifBatch: vi.fn(),
  cancelCompression: vi.fn(() => Promise.resolve()),
  cancelAll: vi.fn(() => Promise.resolve()),
  resetCancel: vi.fn(() => Promise.resolve()),
  clearThumbnailCache: vi.fn(() => Promise.resolve()),
};
vi.mock("../../src/lib/commands", () => commands);

const { useCompressionStore, deriveSummary } = await import("../../src/stores/compressionStore");
const controller = await import("../../src/lib/compressionController");

import type { BatchEntry } from "../../src/lib/commands";
import type { CompressionResult, ProgressEvent, QueuedFile } from "../../src/types/compression";

let counter = 0;
function makeFile(overrides: Partial<QueuedFile> = {}): QueuedFile {
  const id = `f${++counter}`;
  return {
    id,
    path: `/in/${id}.mp4`,
    name: `${id}.mp4`,
    size: 100 + counter,
    mediaType: "video",
    status: "queued",
    progress: 0,
    duration: 12.5,
    ...overrides,
  };
}

function okResult(entry: BatchEntry, jobId: string): CompressionResult {
  return {
    jobId, inputPath: entry.input, outputPath: entry.output, inputSize: 100, outputSize: 50,
    durationMs: 5, success: true, error: null,
  };
}

/** Simulate a backend that emits Started/Progress/Completed for each entry. */
function backendThatCompletes(percentSteps: number[] = [50]) {
  return vi.fn(async (entries: BatchEntry[], _opts: unknown, channel: FakeChannel<ProgressEvent>) => {
    const results: CompressionResult[] = [];
    entries.forEach((entry, i) => {
      const jobId = `job-${entry.input}-${i}`;
      channel.onmessage({ event: "started", data: { jobId, fileName: entry.input, inputPath: entry.input } });
      for (const p of percentSteps) {
        channel.onmessage({
          event: "progress",
          data: { jobId, fileName: entry.input, percent: p, currentFrame: null, totalFrames: null, speed: null, etaSeconds: null },
        });
      }
      const result = okResult(entry, jobId);
      channel.onmessage({ event: "completed", data: result });
      results.push(result);
    });
    return results;
  });
}

const store = () => useCompressionStore.getState();

beforeEach(() => {
  vi.clearAllMocks();
  useCompressionStore.setState({
    files: [],
    summary: deriveSummary([]),
    filesRevision: 0,
    isCompressing: false,
    _activeOps: 0,
    isPaused: false,
    _cancelRequested: false,
    outputMode: "sameDir",
    outputDir: null,
    outputTemplate: "{name}_compressed",
  });
  commands.compressVideosBatch.mockImplementation(backendThatCompletes());
  commands.compressImagesBatch.mockImplementation(backendThatCompletes());
  commands.compressPdfsBatch.mockImplementation(backendThatCompletes());
  commands.compressAudioBatch.mockImplementation(backendThatCompletes());
});

describe("startCompression", () => {
  it("routes each media type to its batch, passes duration, and completes all files", async () => {
    const v = makeFile();
    const i = makeFile({ mediaType: "image", path: "/in/pic.png", duration: null });
    const p = makeFile({ mediaType: "pdf", path: "/in/doc.pdf", duration: undefined });
    const a = makeFile({ mediaType: "audio", path: "/in/song.mp3", duration: 30 });
    store().addFiles([v, i, p, a]);

    await controller.startCompression();

    expect(commands.compressVideosBatch).toHaveBeenCalledTimes(1);
    expect(commands.compressImagesBatch).toHaveBeenCalledTimes(1);
    expect(commands.compressPdfsBatch).toHaveBeenCalledTimes(1);
    expect(commands.compressAudioBatch).toHaveBeenCalledTimes(1);

    const videoEntries: BatchEntry[] = commands.compressVideosBatch.mock.calls[0][0];
    expect(videoEntries).toEqual([{ input: v.path, output: "/in/f1_compressed.mp4", duration: 12.5 }]);
    const audioEntries: BatchEntry[] = commands.compressAudioBatch.mock.calls[0][0];
    expect(audioEntries[0].duration).toBe(30);
    expect(audioEntries[0].output).toBe("/in/song_compressed.mp3");
    const imageEntries: BatchEntry[] = commands.compressImagesBatch.mock.calls[0][0];
    expect(imageEntries[0].duration).toBeNull();
    expect(imageEntries[0].output).toBe("/in/pic_compressed.jpeg");

    expect(store().files.every((f) => f.status === "complete")).toBe(true);
    expect(store().summary).toMatchObject({ complete: 4, queued: 0, processing: 0 });
    expect(store().isCompressing).toBe(false);
  });

  it("uses one timestamp for every file in a batch", async () => {
    useCompressionStore.setState({ outputTemplate: "{name}_{time}" });
    store().addFiles([makeFile(), makeFile(), makeFile()]);
    await controller.startCompression();
    const entries: BatchEntry[] = commands.compressVideosBatch.mock.calls[0][0];
    const stamps = new Set(entries.map((e) => e.output.split("_").pop()));
    expect(stamps.size).toBe(1);
  });

  it("picks up files added while a batch is running", async () => {
    const first = makeFile();
    const late = makeFile();
    store().addFiles([first]);
    let call = 0;
    commands.compressVideosBatch.mockImplementation(async (entries: BatchEntry[], opts: unknown, ch: FakeChannel<ProgressEvent>) => {
      call += 1;
      if (call === 1) store().addFiles([late]);
      return backendThatCompletes()(entries, opts, ch);
    });

    await controller.startCompression();

    expect(commands.compressVideosBatch).toHaveBeenCalledTimes(2);
    expect(store().files.map((f) => f.status)).toEqual(["complete", "complete"]);
  });

  it("marks every non-terminal file failed when the batch invoke rejects", async () => {
    const a = makeFile();
    const b = makeFile();
    store().addFiles([a, b]);
    commands.compressVideosBatch.mockRejectedValue(new Error("validation failed"));

    await controller.startCompression();

    expect(store().files.map((f) => f.status)).toEqual(["error", "error"]);
    expect(store().files[0].error).toContain("validation failed");
    // No second iteration: nothing is queued any more.
    expect(commands.compressVideosBatch).toHaveBeenCalledTimes(1);
  });

  it("reconciles setup-level failures reported only in the result list", async () => {
    const a = makeFile();
    store().addFiles([a]);
    commands.compressVideosBatch.mockImplementation(async (entries: BatchEntry[]) =>
      entries.map((e) => ({ ...okResult(e, "j"), success: false, error: "spawn failed" })),
    );

    await controller.startCompression();

    expect(store().files[0].status).toBe("error");
    expect(store().files[0].error).toBe("spawn failed");
  });

  it("stops instead of spinning when the backend leaves the queue untouched", async () => {
    store().addFiles([makeFile()]);
    commands.compressVideosBatch.mockResolvedValue([]);
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});

    await controller.startCompression();

    // One attempt, then the unchanged-queue guard stops the drain loop.
    expect(commands.compressVideosBatch).toHaveBeenCalledTimes(1);
    expect(warn).toHaveBeenCalled();
    expect(store().files[0].status).toBe("queued");
    expect(store().isCompressing).toBe(false);
    warn.mockRestore();
  });

  it("refuses to start when a custom output dir is required but unset", async () => {
    useCompressionStore.setState({ outputMode: "customDir", outputDir: null });
    store().addFiles([makeFile()]);
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    await controller.startCompression();
    expect(commands.compressVideosBatch).not.toHaveBeenCalled();
    error.mockRestore();
  });

  it("honors Cancel All: leaves files queued and does not mark them failed", async () => {
    const a = makeFile();
    store().addFiles([a]);
    commands.compressVideosBatch.mockImplementation(async (entries: BatchEntry[], _o: unknown, ch: FakeChannel<ProgressEvent>) => {
      ch.onmessage({ event: "started", data: { jobId: "j1", fileName: "x", inputPath: entries[0].input } });
      await controller.cancelAllCompression();
      throw new Error("killed");
    });

    await controller.startCompression();

    expect(commands.cancelAll).toHaveBeenCalled();
    expect(store().files[0].status).toBe("queued");
    expect(store().isCompressing).toBe(false);
  });
});

describe("tools", () => {
  it("extractAudioFromFile passes the probed duration and completes the file", async () => {
    const v = makeFile({ duration: 42 });
    store().addFiles([v]);
    commands.extractAudio.mockImplementation(async (input: string, output: string, _o: unknown, ch: FakeChannel<ProgressEvent>, duration: number) => {
      expect(duration).toBe(42);
      expect(output).toBe("/in/f1_compressed.mp3".replace("f1", v.name.replace(".mp4", "")));
      ch.onmessage({ event: "started", data: { jobId: "j", fileName: input, inputPath: input } });
      const r = okResult({ input, output }, "j");
      ch.onmessage({ event: "completed", data: r });
      return r;
    });

    await controller.extractAudioFromFile(v);

    expect(commands.extractAudio).toHaveBeenCalledTimes(1);
    expect(store().files[0].status).toBe("complete");
    expect(store().isCompressing).toBe(false);
  });

  it("convertAllToGif only touches queued videos", async () => {
    const v1 = makeFile();
    const v2 = makeFile({ status: "complete" });
    const img = makeFile({ mediaType: "image", path: "/in/x.png" });
    store().addFiles([v1, v2, img]);
    commands.convertVideosToGifBatch.mockImplementation(backendThatCompletes());

    await controller.convertAllToGif();

    const entries: BatchEntry[] = commands.convertVideosToGifBatch.mock.calls[0][0];
    expect(entries.map((e) => e.input)).toEqual([v1.path]);
    expect(entries[0].output).toBe("/in/f1_compressed.gif".replace("f1", v1.name.replace(".mp4", "")));
  });

  it("cancelProcessingFiles cancels every processing job in parallel", async () => {
    const a = makeFile({ status: "processing", jobId: "ja" });
    const b = makeFile({ status: "processing", jobId: "jb" });
    const c = makeFile();
    useCompressionStore.setState({ files: [a, b, c], summary: deriveSummary([a, b, c]) });

    await controller.cancelProcessingFiles();

    expect(commands.cancelCompression).toHaveBeenCalledTimes(2);
    expect(store().files.map((f) => f.status)).toEqual(["error", "error", "queued"]);
  });
});
