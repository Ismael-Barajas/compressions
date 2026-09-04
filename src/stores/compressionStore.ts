import { create } from "zustand";
import { clearThumbnailCache } from "../lib/commands";
import type {
  QueuedFile,
  VideoOptions,
  ImageOptions,
  AudioExtractionOptions,
  AudioCompressionOptions,
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
  resizeMode: "fit",
  stripMetadata: true,
};

const DEFAULT_AUDIO_OPTIONS: AudioExtractionOptions = {
  format: "Mp3",
  bitrate: "192k",
  sampleRate: null,
};

const DEFAULT_GIF_OPTIONS: GifConversionOptions = {
  fps: 12,
  width: 480,
  maxColors: 128,
  dither: "bayer",
};

const DEFAULT_PDF_OPTIONS: PdfOptions = {
  quality: "ebook",
  dpi: null,
};

const DEFAULT_AUDIO_COMPRESSION_OPTIONS: AudioCompressionOptions = {
  format: "Original",
  bitrate: "192k",
  sampleRate: null,
};

export type SidebarTab = "compress" | "tools";
export type CompressTab = "video" | "image" | "pdf" | "audio";

/**
 * Aggregates over `files` that components need constantly. Kept up to date by
 * every action that touches files so components can subscribe to a primitive
 * instead of re-scanning the whole array on each progress tick.
 */
export interface FilesSummary {
  total: number;
  queued: number;
  processing: number;
  complete: number;
  error: number;
  /** Sum of `progress` over files currently processing (for the batch bar). */
  progressSum: number;
  video: number;
  image: number;
  pdf: number;
  audio: number;
  queuedVideos: number;
}

const EMPTY_SUMMARY: FilesSummary = {
  total: 0,
  queued: 0,
  processing: 0,
  complete: 0,
  error: 0,
  progressSum: 0,
  video: 0,
  image: 0,
  pdf: 0,
  audio: 0,
  queuedVideos: 0,
};

export function deriveSummary(files: QueuedFile[]): FilesSummary {
  const s: FilesSummary = { ...EMPTY_SUMMARY, total: files.length };
  for (const f of files) {
    s[f.status] += 1;
    s[f.mediaType] += 1;
    if (f.status === "processing") s.progressSum += f.progress;
    if (f.status === "queued" && f.mediaType === "video") s.queuedVideos += 1;
  }
  return s;
}

export interface FileProbeInfo {
  size: number;
  resolution?: Resolution | null;
  duration?: number | null;
}

interface CompressionStore {
  files: QueuedFile[];
  /** Derived counts; see `FilesSummary`. */
  summary: FilesSummary;
  /** Bumped whenever files are added or removed (not on per-file updates). */
  filesRevision: number;
  videoOptions: VideoOptions;
  imageOptions: ImageOptions;
  audioOptions: AudioExtractionOptions;
  audioCompressionOptions: AudioCompressionOptions;
  gifOptions: GifConversionOptions;
  pdfOptions: PdfOptions;
  outputDir: string | null;
  outputMode: OutputMode;
  subfolderName: string;
  outputTemplate: string;
  activePreset: string | null;
  activeSidebarTab: SidebarTab;
  activeCompressTab: CompressTab | null;
  theme: "light" | "dark";
  showThumbnails: boolean;
  isCompressing: boolean;
  _activeOps: number;
  isPaused: boolean;
  // Internal flag the drain loop in the controller checks each iteration to bail out.
  _cancelRequested: boolean;

  addFiles: (files: QueuedFile[]) => void;
  removeFile: (id: string) => void;
  clearFiles: () => void;
  updateProgress: (jobId: string, payload: ProgressPayload) => void;
  markComplete: (jobId: string, result: CompressionResult) => void;
  markError: (jobId: string, message: string) => void;
  /** Mark several files (by id) as failed in one state update. */
  markErrorByIds: (ids: Iterable<string>, message: string, onlyIfStatus?: QueuedFile["status"][]) => void;
  setFileStatus: (id: string, status: QueuedFile["status"], jobId?: string) => void;
  updateFileProbe: (id: string, info: FileProbeInfo) => void;
  updateFileProbes: (updates: Array<{ id: string; info: FileProbeInfo }>) => void;
  setThumbnailPath: (id: string, thumbnailPath: string) => void;
  setVideoOptions: (opts: Partial<VideoOptions>) => void;
  setImageOptions: (opts: Partial<ImageOptions>) => void;
  setAudioOptions: (opts: Partial<AudioExtractionOptions>) => void;
  setAudioCompressionOptions: (opts: Partial<AudioCompressionOptions>) => void;
  setGifOptions: (opts: Partial<GifConversionOptions>) => void;
  setPdfOptions: (opts: Partial<PdfOptions>) => void;
  setOutputDir: (dir: string | null) => void;
  setOutputMode: (mode: OutputMode) => void;
  setSubfolderName: (name: string) => void;
  setOutputTemplate: (template: string) => void;
  setActivePreset: (id: string | null) => void;
  applyPreset: (id: string, options: { video?: VideoOptions; image?: ImageOptions }) => void;
  setActiveSidebarTab: (tab: SidebarTab) => void;
  setActiveCompressTab: (tab: CompressTab | null) => void;
  toggleTheme: () => void;
  toggleThumbnails: () => void;
  setIsCompressing: (value: boolean) => void;
  startOperation: () => void;
  endOperation: () => void;
  retryFile: (id: string) => void;
  pauseCompression: () => void;
  resumeCompression: () => void;
  // Reverts every processing/queued file back to a fresh queued state and signals
  // the drain loop to bail. Does NOT kill child processes — callers must invoke
  // the cancel_all Tauri command alongside this for full cancellation.
  cancelAllCompression: () => void;
  // Clears pause/cancel flags. Called at the start of a fresh drain.
  resetQueueControlFlags: () => void;
}

function getInitialTheme(): "light" | "dark" {
  if (typeof window !== "undefined") {
    const stored = localStorage.getItem("compressions-theme");
    if (stored === "light" || stored === "dark") return stored;
    if (window.matchMedia("(prefers-color-scheme: dark)").matches) return "dark";
  }
  return "light";
}

function getStoredThumbnails(): boolean {
  if (typeof window !== "undefined") {
    return localStorage.getItem("compressions-show-thumbnails") === "true";
  }
  return false;
}

function getStoredTemplate(): string {
  if (typeof window !== "undefined") {
    return localStorage.getItem("compressions-output-template") || "{name}_compressed";
  }
  return "{name}_compressed";
}

// --- O(1) lookups -----------------------------------------------------------
// Index maps live outside the reactive state: they are caches over `files` and
// are validated against it on every hit, so a `setState({ files })` from tests or
// devtools simply causes a rebuild on the next lookup.

let indexById = new Map<string, number>();
let fileIdByJobId = new Map<string, string>();
let indexedFiles: QueuedFile[] | null = null;

function rebuildIndex(files: QueuedFile[]) {
  indexById = new Map();
  fileIdByJobId = new Map();
  files.forEach((f, i) => {
    indexById.set(f.id, i);
    if (f.jobId) fileIdByJobId.set(f.jobId, f.id);
  });
  indexedFiles = files;
}

function ensureIndex(files: QueuedFile[]) {
  if (indexedFiles !== files) rebuildIndex(files);
}

function findIndexById(files: QueuedFile[], id: string): number {
  ensureIndex(files);
  const idx = indexById.get(id);
  return idx !== undefined && files[idx]?.id === id ? idx : -1;
}

function findIndexByJobId(files: QueuedFile[], jobId: string): number {
  ensureIndex(files);
  const id = fileIdByJobId.get(jobId);
  if (id !== undefined) {
    const idx = findIndexById(files, id);
    if (idx >= 0 && files[idx].jobId === jobId) return idx;
  }
  // Slow path (only when the map is stale): scan, then remember.
  const idx = files.findIndex((f) => f.jobId === jobId);
  if (idx >= 0) fileIdByJobId.set(jobId, files[idx].id);
  return idx;
}

/** Copy `files` with one element replaced; keeps the index maps valid for the copy. */
function replaceAt(files: QueuedFile[], idx: number, next: QueuedFile): QueuedFile[] {
  const out = files.slice();
  out[idx] = next;
  ensureIndex(files);
  const prev = files[idx];
  if (prev.jobId && prev.jobId !== next.jobId) fileIdByJobId.delete(prev.jobId);
  if (next.jobId) fileIdByJobId.set(next.jobId, next.id);
  indexedFiles = out;
  return out;
}

/** Adjust the summary for a single file's status/progress transition. */
function summaryAfterTransition(
  summary: FilesSummary,
  prev: QueuedFile,
  next: QueuedFile,
): FilesSummary {
  const s = { ...summary };
  if (prev.status !== next.status) {
    s[prev.status] -= 1;
    s[next.status] += 1;
    if (prev.mediaType === "video") {
      if (prev.status === "queued") s.queuedVideos -= 1;
      if (next.status === "queued") s.queuedVideos += 1;
    }
  }
  const prevContribution = prev.status === "processing" ? prev.progress : 0;
  const nextContribution = next.status === "processing" ? next.progress : 0;
  s.progressSum += nextContribution - prevContribution;
  return s;
}

const RESET_FOR_QUEUE = {
  status: "queued" as const,
  progress: 0,
  jobId: undefined,
  error: undefined,
  result: undefined,
};

export const useCompressionStore = create<CompressionStore>((set) => ({
  files: [],
  summary: EMPTY_SUMMARY,
  filesRevision: 0,
  videoOptions: DEFAULT_VIDEO_OPTIONS,
  imageOptions: DEFAULT_IMAGE_OPTIONS,
  audioOptions: DEFAULT_AUDIO_OPTIONS,
  audioCompressionOptions: DEFAULT_AUDIO_COMPRESSION_OPTIONS,
  gifOptions: DEFAULT_GIF_OPTIONS,
  pdfOptions: DEFAULT_PDF_OPTIONS,
  outputDir: null,
  outputMode: "sameDir",
  subfolderName: "compressed",
  outputTemplate: getStoredTemplate(),
  activePreset: null,
  activeSidebarTab: "compress",
  activeCompressTab: null,
  theme: getInitialTheme(),
  showThumbnails: getStoredThumbnails(),
  isCompressing: false,
  _activeOps: 0,
  isPaused: false,
  _cancelRequested: false,

  addFiles: (newFiles) =>
    set((state) => {
      const existingPaths = new Set(state.files.map((f) => f.path));
      const unique = newFiles.filter((f) => !existingPaths.has(f.path));
      if (unique.length === 0) return {};
      const files = [...state.files, ...unique];
      return { files, summary: deriveSummary(files), filesRevision: state.filesRevision + 1 };
    }),

  removeFile: (id) =>
    set((state) => {
      const files = state.files.filter((f) => f.id !== id);
      if (files.length === state.files.length) return {};
      return { files, summary: deriveSummary(files), filesRevision: state.filesRevision + 1 };
    }),

  clearFiles: () => {
    clearThumbnailCache().catch(() => {});
    return set((state) => ({
      files: [],
      summary: EMPTY_SUMMARY,
      filesRevision: state.filesRevision + 1,
      isCompressing: false,
      _activeOps: 0,
    }));
  },

  setThumbnailPath: (id, thumbnailPath) =>
    set((state) => {
      const idx = findIndexById(state.files, id);
      if (idx < 0) return {};
      return { files: replaceAt(state.files, idx, { ...state.files[idx], thumbnailPath }) };
    }),

  updateFileProbe: (id, info) =>
    set((state) => {
      const idx = findIndexById(state.files, id);
      if (idx < 0) return {};
      const f = state.files[idx];
      return {
        files: replaceAt(state.files, idx, {
          ...f,
          size: info.size,
          resolution: info.resolution ?? f.resolution,
          duration: info.duration ?? f.duration,
        }),
      };
    }),

  updateFileProbes: (updates) =>
    set((state) => {
      const updateMap = new Map(updates.map((u) => [u.id, u.info]));
      const files = state.files.map((f) => {
        const info = updateMap.get(f.id);
        if (!info) return f;
        return {
          ...f,
          size: info.size,
          resolution: info.resolution ?? f.resolution,
          duration: info.duration ?? f.duration,
        };
      });
      return { files };
    }),

  updateProgress: (jobId, payload) =>
    set((state) => {
      const idx = findIndexByJobId(state.files, jobId);
      if (idx < 0) return {};
      const prev = state.files[idx];
      if (prev.progress === payload.percent) return {};
      const next = { ...prev, progress: payload.percent };
      return {
        files: replaceAt(state.files, idx, next),
        summary: summaryAfterTransition(state.summary, prev, next),
      };
    }),

  markComplete: (jobId, result) =>
    set((state) => {
      const idx = findIndexByJobId(state.files, jobId);
      if (idx < 0) return {};
      const prev = state.files[idx];
      const next = { ...prev, status: "complete" as const, progress: 100, result };
      return {
        files: replaceAt(state.files, idx, next),
        summary: summaryAfterTransition(state.summary, prev, next),
      };
    }),

  markError: (jobId, message) =>
    set((state) => {
      const idx = findIndexByJobId(state.files, jobId);
      if (idx < 0) return {};
      const prev = state.files[idx];
      const next = { ...prev, status: "error" as const, error: message };
      return {
        files: replaceAt(state.files, idx, next),
        summary: summaryAfterTransition(state.summary, prev, next),
      };
    }),

  markErrorByIds: (ids, message, onlyIfStatus) =>
    set((state) => {
      const idSet = new Set(ids);
      if (idSet.size === 0) return {};
      let changed = false;
      const files = state.files.map((f) => {
        if (!idSet.has(f.id)) return f;
        if (onlyIfStatus && !onlyIfStatus.includes(f.status)) return f;
        changed = true;
        return { ...f, status: "error" as const, error: message };
      });
      if (!changed) return {};
      return { files, summary: deriveSummary(files) };
    }),

  setFileStatus: (id, status, jobId) =>
    set((state) => {
      const idx = findIndexById(state.files, id);
      if (idx < 0) return {};
      const prev = state.files[idx];
      const next = { ...prev, status, ...(jobId ? { jobId } : {}) };
      return {
        files: replaceAt(state.files, idx, next),
        summary: summaryAfterTransition(state.summary, prev, next),
      };
    }),

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

  setAudioCompressionOptions: (opts) =>
    set((state) => ({
      audioCompressionOptions: { ...state.audioCompressionOptions, ...opts },
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

  applyPreset: (id, options) =>
    set((state) => ({
      activePreset: id,
      videoOptions: options.video ?? state.videoOptions,
      imageOptions: options.image ?? state.imageOptions,
    })),

  setActiveSidebarTab: (tab) => set({ activeSidebarTab: tab }),

  setActiveCompressTab: (tab) => set({ activeCompressTab: tab }),

  toggleThumbnails: () =>
    set((state) => {
      const next = !state.showThumbnails;
      localStorage.setItem("compressions-show-thumbnails", String(next));
      return { showThumbnails: next };
    }),

  toggleTheme: () =>
    set((state) => {
      const next = state.theme === "light" ? "dark" : "light";
      localStorage.setItem("compressions-theme", next);
      return { theme: next };
    }),

  setIsCompressing: (value) => set({ isCompressing: value }),

  startOperation: () =>
    set((state) => {
      const next = state._activeOps + 1;
      return { _activeOps: next, isCompressing: next > 0 };
    }),

  endOperation: () =>
    set((state) => {
      const next = Math.max(0, state._activeOps - 1);
      return { _activeOps: next, isCompressing: next > 0 };
    }),

  retryFile: (id) =>
    set((state) => {
      const idx = findIndexById(state.files, id);
      if (idx < 0) return {};
      const prev = state.files[idx];
      const next = { ...prev, ...RESET_FOR_QUEUE };
      return {
        files: replaceAt(state.files, idx, next),
        summary: summaryAfterTransition(state.summary, prev, next),
      };
    }),

  pauseCompression: () => set({ isPaused: true }),

  resumeCompression: () => set({ isPaused: false }),

  cancelAllCompression: () =>
    set((state) => {
      const files = state.files.map((f) =>
        f.status === "processing" || f.status === "queued" ? { ...f, ...RESET_FOR_QUEUE } : f,
      );
      return {
        _cancelRequested: true,
        isPaused: false,
        files,
        summary: deriveSummary(files),
      };
    }),

  resetQueueControlFlags: () => set({ isPaused: false, _cancelRequested: false }),
}));

export {
  DEFAULT_VIDEO_OPTIONS,
  DEFAULT_IMAGE_OPTIONS,
  DEFAULT_AUDIO_OPTIONS,
  DEFAULT_AUDIO_COMPRESSION_OPTIONS,
  DEFAULT_GIF_OPTIONS,
  DEFAULT_PDF_OPTIONS,
};
