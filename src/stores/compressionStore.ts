import { create } from "zustand";
import type {
  QueuedFile,
  VideoOptions,
  ImageOptions,
  AudioExtractionOptions,
  GifConversionOptions,
  PdfOptions,
  CompressionResult,
  ProgressPayload,
  OutputMode,
  Resolution,
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

const DEFAULT_AUDIO_OPTIONS: AudioExtractionOptions = {
  format: "Mp3",
  bitrate: "192k",
  sampleRate: null,
};

const DEFAULT_GIF_OPTIONS: GifConversionOptions = {
  fps: 15,
  width: null,
  maxColors: 256,
  dither: "floyd_steinberg",
};

const DEFAULT_PDF_OPTIONS: PdfOptions = {
  quality: "ebook",
  dpi: null,
};

interface CompressionStore {
  files: QueuedFile[];
  videoOptions: VideoOptions;
  imageOptions: ImageOptions;
  audioOptions: AudioExtractionOptions;
  gifOptions: GifConversionOptions;
  pdfOptions: PdfOptions;
  outputDir: string | null;
  outputMode: OutputMode;
  subfolderName: string;
  outputTemplate: string;
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
  updateFileProbe: (id: string, info: { size: number; resolution?: Resolution | null; duration?: number | null }) => void;
  setVideoOptions: (opts: Partial<VideoOptions>) => void;
  setImageOptions: (opts: Partial<ImageOptions>) => void;
  setAudioOptions: (opts: Partial<AudioExtractionOptions>) => void;
  setGifOptions: (opts: Partial<GifConversionOptions>) => void;
  setPdfOptions: (opts: Partial<PdfOptions>) => void;
  setOutputDir: (dir: string | null) => void;
  setOutputMode: (mode: OutputMode) => void;
  setSubfolderName: (name: string) => void;
  setOutputTemplate: (template: string) => void;
  setActivePreset: (id: string | null) => void;
  toggleTheme: () => void;
  setIsCompressing: (value: boolean) => void;
  retryFile: (id: string) => void;
}

function getInitialTheme(): "light" | "dark" {
  if (typeof window !== "undefined") {
    const stored = localStorage.getItem("compressions-theme");
    if (stored === "light" || stored === "dark") return stored;
    if (window.matchMedia("(prefers-color-scheme: dark)").matches) return "dark";
  }
  return "light";
}

function getStoredTemplate(): string {
  if (typeof window !== "undefined") {
    return localStorage.getItem("compressions-output-template") || "{name}_compressed";
  }
  return "{name}_compressed";
}

export const useCompressionStore = create<CompressionStore>((set) => ({
  files: [],
  videoOptions: DEFAULT_VIDEO_OPTIONS,
  imageOptions: DEFAULT_IMAGE_OPTIONS,
  audioOptions: DEFAULT_AUDIO_OPTIONS,
  gifOptions: DEFAULT_GIF_OPTIONS,
  pdfOptions: DEFAULT_PDF_OPTIONS,
  outputDir: null,
  outputMode: "sameDir",
  subfolderName: "compressed",
  outputTemplate: getStoredTemplate(),
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

  clearFiles: () => set({ files: [], isCompressing: false }),

  updateFileProbe: (id, info) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.id === id
          ? { ...f, size: info.size, resolution: info.resolution ?? f.resolution, duration: info.duration ?? f.duration }
          : f,
      ),
    })),

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

  setAudioOptions: (opts) =>
    set((state) => ({
      audioOptions: { ...state.audioOptions, ...opts },
    })),

  setGifOptions: (opts) =>
    set((state) => ({
      gifOptions: { ...state.gifOptions, ...opts },
    })),

  setPdfOptions: (opts) =>
    set((state) => ({
      pdfOptions: { ...state.pdfOptions, ...opts },
    })),

  setOutputDir: (dir) => set({ outputDir: dir }),

  setOutputMode: (mode) => set({ outputMode: mode }),

  setSubfolderName: (name) => set({ subfolderName: name }),

  setOutputTemplate: (template) => {
    localStorage.setItem("compressions-output-template", template);
    set({ outputTemplate: template });
  },

  setActivePreset: (id) => set({ activePreset: id }),

  toggleTheme: () =>
    set((state) => {
      const next = state.theme === "light" ? "dark" : "light";
      localStorage.setItem("compressions-theme", next);
      return { theme: next };
    }),

  setIsCompressing: (value) => set({ isCompressing: value }),

  retryFile: (id) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.id === id
          ? { ...f, status: "queued" as const, progress: 0, error: undefined, result: undefined, jobId: undefined }
          : f,
      ),
    })),
}));

export { DEFAULT_VIDEO_OPTIONS, DEFAULT_IMAGE_OPTIONS, DEFAULT_AUDIO_OPTIONS, DEFAULT_GIF_OPTIONS, DEFAULT_PDF_OPTIONS };
