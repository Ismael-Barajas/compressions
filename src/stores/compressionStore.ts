import { create } from "zustand";
import type {
  QueuedFile,
  VideoOptions,
  ImageOptions,
  CompressionResult,
  ProgressPayload,
} from "../types/compression";

const DEFAULT_VIDEO_OPTIONS: VideoOptions = {
  codec: "H264",
  crf: 23,
  resolution: null,
  bitrate: null,
  framerate: null,
  audioCodec: "AAC",
  audioBitrate: "128k",
};

const DEFAULT_IMAGE_OPTIONS: ImageOptions = {
  format: "Jpeg",
  quality: 80,
  resize: null,
  stripMetadata: true,
};

interface CompressionStore {
  files: QueuedFile[];
  videoOptions: VideoOptions;
  imageOptions: ImageOptions;
  outputDir: string | null;
  sameAsSource: boolean;
  activePreset: string | null;
  theme: "light" | "dark";
  isCompressing: boolean;

  addFiles: (files: QueuedFile[]) => void;
  removeFile: (id: string) => void;
  clearFiles: () => void;
  updateProgress: (jobId: string, payload: ProgressPayload) => void;
  markComplete: (jobId: string, result: CompressionResult) => void;
  markError: (jobId: string, message: string) => void;
  setFileStatus: (id: string, status: QueuedFile["status"], jobId?: string) => void;
  setVideoOptions: (opts: Partial<VideoOptions>) => void;
  setImageOptions: (opts: Partial<ImageOptions>) => void;
  setOutputDir: (dir: string | null) => void;
  setSameAsSource: (value: boolean) => void;
  setActivePreset: (id: string | null) => void;
  toggleTheme: () => void;
  setIsCompressing: (value: boolean) => void;
}

function getInitialTheme(): "light" | "dark" {
  if (typeof window !== "undefined") {
    const stored = localStorage.getItem("compressions-theme");
    if (stored === "light" || stored === "dark") return stored;
    if (window.matchMedia("(prefers-color-scheme: dark)").matches) return "dark";
  }
  return "light";
}

export const useCompressionStore = create<CompressionStore>((set) => ({
  files: [],
  videoOptions: DEFAULT_VIDEO_OPTIONS,
  imageOptions: DEFAULT_IMAGE_OPTIONS,
  outputDir: null,
  sameAsSource: true,
  activePreset: null,
  theme: getInitialTheme(),
  isCompressing: false,

  addFiles: (newFiles) =>
    set((state) => {
      const existingPaths = new Set(state.files.map((f) => f.path));
      const unique = newFiles.filter((f) => !existingPaths.has(f.path));
      return { files: [...state.files, ...unique] };
    }),

  removeFile: (id) =>
    set((state) => ({ files: state.files.filter((f) => f.id !== id) })),

  clearFiles: () => set({ files: [] }),

  updateProgress: (jobId, payload) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.jobId === jobId ? { ...f, progress: payload.percent } : f,
      ),
    })),

  markComplete: (jobId, result) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.jobId === jobId
          ? { ...f, status: "complete" as const, progress: 100, result }
          : f,
      ),
    })),

  markError: (jobId, message) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.jobId === jobId
          ? { ...f, status: "error" as const, error: message }
          : f,
      ),
    })),

  setFileStatus: (id, status, jobId) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.id === id ? { ...f, status, ...(jobId ? { jobId } : {}) } : f,
      ),
    })),

  setVideoOptions: (opts) =>
    set((state) => ({
      videoOptions: { ...state.videoOptions, ...opts },
      activePreset: null,
    })),

  setImageOptions: (opts) =>
    set((state) => ({
      imageOptions: { ...state.imageOptions, ...opts },
      activePreset: null,
    })),

  setOutputDir: (dir) => set({ outputDir: dir }),

  setSameAsSource: (value) => set({ sameAsSource: value }),

  setActivePreset: (id) => set({ activePreset: id }),

  toggleTheme: () =>
    set((state) => {
      const next = state.theme === "light" ? "dark" : "light";
      localStorage.setItem("compressions-theme", next);
      return { theme: next };
    }),

  setIsCompressing: (value) => set({ isCompressing: value }),
}));

export { DEFAULT_VIDEO_OPTIONS, DEFAULT_IMAGE_OPTIONS };
