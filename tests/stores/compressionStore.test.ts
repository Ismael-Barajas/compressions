/**
 * @vitest-environment jsdom
 */
import { describe, it, expect, beforeEach, vi } from "vitest";

// Mock localStorage before importing the store
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: vi.fn((key: string) => store[key] ?? null),
    setItem: vi.fn((key: string, value: string) => { store[key] = value; }),
    removeItem: vi.fn((key: string) => { delete store[key]; }),
    clear: vi.fn(() => { store = {}; }),
    get length() { return Object.keys(store).length; },
    key: vi.fn((i: number) => Object.keys(store)[i] ?? null),
  };
})();

Object.defineProperty(globalThis, "localStorage", { value: localStorageMock, writable: true });

// Mock window.matchMedia for theme detection
Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

const { useCompressionStore } = await import("../../src/stores/compressionStore");

import type { QueuedFile } from "../../src/types/compression";

function makeFile(overrides: Partial<QueuedFile> = {}): QueuedFile {
  const id = Math.random().toString(36).slice(2);
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

describe("compressionStore", () => {
  beforeEach(() => {
    useCompressionStore.setState({ files: [], isCompressing: false });
  });

  describe("addFiles", () => {
    it("adds files to the queue", () => {
      const files = [makeFile(), makeFile()];
      useCompressionStore.getState().addFiles(files);
      expect(useCompressionStore.getState().files).toHaveLength(2);
    });

    it("deduplicates by path", () => {
      const f1 = makeFile({ path: "/same/path.mp4" });
      const f2 = makeFile({ path: "/same/path.mp4" });
      useCompressionStore.getState().addFiles([f1]);
      useCompressionStore.getState().addFiles([f2]);
      expect(useCompressionStore.getState().files).toHaveLength(1);
    });

    it("allows different paths", () => {
      const f1 = makeFile({ path: "/a.mp4" });
      const f2 = makeFile({ path: "/b.mp4" });
      useCompressionStore.getState().addFiles([f1]);
      useCompressionStore.getState().addFiles([f2]);
      expect(useCompressionStore.getState().files).toHaveLength(2);
    });
  });

  describe("state transitions", () => {
    it("queued -> processing", () => {
      const f = makeFile();
      useCompressionStore.getState().addFiles([f]);
      useCompressionStore.getState().setFileStatus(f.id, "processing", "job-1");

      const file = useCompressionStore.getState().files[0];
      expect(file.status).toBe("processing");
      expect(file.jobId).toBe("job-1");
    });

    it("processing -> complete via markComplete", () => {
      const f = makeFile();
      useCompressionStore.getState().addFiles([f]);
      useCompressionStore.getState().setFileStatus(f.id, "processing", "job-1");
      useCompressionStore.getState().markComplete("job-1", {
        jobId: "job-1",
        inputPath: f.path,
        outputPath: "/out.mp4",
        inputSize: 1024,
        outputSize: 512,
        durationMs: 100,
        success: true,
        error: null,
      });

      const file = useCompressionStore.getState().files[0];
      expect(file.status).toBe("complete");
      expect(file.progress).toBe(100);
      expect(file.result?.outputSize).toBe(512);
    });

    it("processing -> error via markError", () => {
      const f = makeFile();
      useCompressionStore.getState().addFiles([f]);
      useCompressionStore.getState().setFileStatus(f.id, "processing", "job-1");
      useCompressionStore.getState().markError("job-1", "FFmpeg crashed");

      const file = useCompressionStore.getState().files[0];
      expect(file.status).toBe("error");
      expect(file.error).toBe("FFmpeg crashed");
    });
  });

  describe("retryFile", () => {
    it("resets error file to queued", () => {
      const f = makeFile();
      useCompressionStore.getState().addFiles([f]);
      useCompressionStore.setState((s) => ({
        files: s.files.map((file) =>
          file.id === f.id
            ? { ...file, status: "error" as const, error: "failed", jobId: "old-job" }
            : file
        ),
      }));
      useCompressionStore.getState().retryFile(f.id);

      const file = useCompressionStore.getState().files[0];
      expect(file.status).toBe("queued");
      expect(file.progress).toBe(0);
      expect(file.error).toBeUndefined();
      expect(file.jobId).toBeUndefined();
    });
  });

  describe("updateProgress", () => {
    it("updates progress by jobId", () => {
      const f = makeFile();
      useCompressionStore.getState().addFiles([f]);
      useCompressionStore.getState().setFileStatus(f.id, "processing", "job-1");
      useCompressionStore.getState().updateProgress("job-1", {
        jobId: "job-1",
        fileName: f.name,
        percent: 55,
        currentFrame: null,
        totalFrames: null,
        speed: null,
        etaSeconds: null,
      });

      expect(useCompressionStore.getState().files[0].progress).toBe(55);
    });

    it("does not affect other files", () => {
      const f1 = makeFile();
      const f2 = makeFile();
      useCompressionStore.getState().addFiles([f1, f2]);
      useCompressionStore.getState().setFileStatus(f1.id, "processing", "job-1");
      useCompressionStore.getState().updateProgress("job-1", {
        jobId: "job-1",
        fileName: f1.name,
        percent: 75,
        currentFrame: null,
        totalFrames: null,
        speed: null,
        etaSeconds: null,
      });

      expect(useCompressionStore.getState().files[1].progress).toBe(0);
    });
  });

  describe("theme", () => {
    it("toggles between light and dark", () => {
      useCompressionStore.setState({ theme: "light" });
      useCompressionStore.getState().toggleTheme();
      expect(useCompressionStore.getState().theme).toBe("dark");
      useCompressionStore.getState().toggleTheme();
      expect(useCompressionStore.getState().theme).toBe("light");
    });
  });
});
