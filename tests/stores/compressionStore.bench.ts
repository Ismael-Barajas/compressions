import { bench, describe, vi } from "vitest";

// Mock localStorage + matchMedia before import
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

const { useCompressionStore } = await import("../../src/stores/compressionStore");

import type { QueuedFile } from "../../src/types/compression";

function makeFile(i: number): QueuedFile {
  return {
    id: `id-${i}`,
    path: `/path/file-${i}.mp4`,
    name: `file-${i}.mp4`,
    size: 1024,
    mediaType: "video",
    status: "queued",
    progress: 0,
  };
}

describe("compressionStore benchmarks", () => {
  bench("addFiles with 1,000 files", () => {
    useCompressionStore.setState({ files: [] });
    const files = Array.from({ length: 1000 }, (_, i) => makeFile(i));
    useCompressionStore.getState().addFiles(files);
  });

  bench("updateProgress on 500-file store", () => {
    const files = Array.from({ length: 500 }, (_, i) => ({
      ...makeFile(i),
      status: "processing" as const,
      jobId: `job-${i}`,
    }));
    useCompressionStore.setState({ files });
    useCompressionStore.getState().updateProgress("job-250", {
      jobId: "job-250",
      fileName: "file-250.mp4",
      percent: 50,
      currentFrame: null,
      totalFrames: null,
      speed: null,
      etaSeconds: null,
    });
  });
});
