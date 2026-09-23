/**
 * Drives compression work: builds batches, wires progress channels, and runs the
 * drain loop. Plain functions over the zustand store (no React), so any number of
 * components can call them without each creating subscriptions.
 */
import { Channel } from "@tauri-apps/api/core";
import {
  compressVideosBatch,
  compressImagesBatch,
  compressPdfsBatch,
  compressAudioBatch,
  extractAudio,
  extractAudioBatch,
  convertVideoToGif,
  convertVideosToGifBatch,
  cancelCompression,
  cancelAll as cancelAllBackend,
  resetCancel,
  type BatchEntry,
} from "./commands";
import { useCompressionStore } from "../stores/compressionStore";
import {
  buildOutputPath,
  getAudioExtension,
  resolveAudioCompressionExtension,
  resolveImageOutputFormat,
  resolveOutputDir,
  templateStamp,
} from "./fileUtils";
import { sortQueuedFilesForCompression } from "./scheduling";
import type { CompressionResult, MediaType, ProgressEvent, QueuedFile } from "../types/compression";

const store = useCompressionStore;

const MEDIA_TYPES: MediaType[] = ["video", "image", "pdf", "audio"];

/** Progress channel that routes backend events to the store for a known batch. */
export function createProgressChannel(files: QueuedFile[]): Channel<ProgressEvent> {
  const idByPath = new Map(files.map((f) => [f.path, f.id]));
  const { setFileStatus, updateProgress, markComplete, markError } = store.getState();
  const channel = new Channel<ProgressEvent>();
  channel.onmessage = (event: ProgressEvent) => {
    switch (event.event) {
      case "started": {
        const id = idByPath.get(event.data.inputPath);
        if (id) setFileStatus(id, "processing", event.data.jobId);
        break;
      }
      case "progress":
        updateProgress(event.data.jobId, event.data);
        break;
      case "completed":
        markComplete(event.data.jobId, event.data);
        break;
      case "error":
        markError(event.data.jobId, event.data.message);
        break;
    }
  };
  return channel;
}

interface BatchRun {
  files: QueuedFile[];
  entries: BatchEntry[];
  invoke: (entries: BatchEntry[], channel: Channel<ProgressEvent>) => Promise<CompressionResult[]>;
}

const NON_TERMINAL: QueuedFile["status"][] = ["queued", "processing"];

/**
 * Run one media-type batch and reconcile the outcome:
 * - if the invoke itself rejects, every file that never reached a terminal state is
 *   marked failed in a single store update;
 * - if it resolves, results for files that received no `Completed`/`Error` event
 *   (setup-level failures happen before `Started`) are applied from the returned list.
 * Skips reconciliation after Cancel All, which already reset the files.
 */
export async function runBatch({ files, entries, invoke }: BatchRun): Promise<void> {
  if (files.length === 0) return;
  const channel = createProgressChannel(files);
  const idByPath = new Map(files.map((f) => [f.path, f.id]));

  let results: CompressionResult[] = [];
  try {
    results = await invoke(entries, channel);
  } catch (err) {
    if (store.getState()._cancelRequested) return;
    store.getState().markErrorByIds(idByPath.values(), String(err), NON_TERMINAL);
    return;
  }

  if (store.getState()._cancelRequested) return;
  const current = new Map(store.getState().files.map((f) => [f.id, f]));
  const unresolved: string[] = [];
  let message = "Failed";
  for (const result of results) {
    if (result.success) continue;
    const id = idByPath.get(result.inputPath);
    const file = id ? current.get(id) : undefined;
    if (file && NON_TERMINAL.includes(file.status)) {
      unresolved.push(file.id);
      message = result.error ?? message;
    }
  }
  if (unresolved.length > 0) {
    store.getState().markErrorByIds(unresolved, message, NON_TERMINAL);
  }
}

function outputSettings() {
  const { outputMode, outputDir, subfolderName, outputTemplate } = store.getState();
  return { outputMode, outputDir, subfolderName, outputTemplate };
}

function entriesFor(
  files: QueuedFile[],
  format: (f: QueuedFile) => string | undefined,
): BatchEntry[] {
  const settings = outputSettings();
  const stamp = templateStamp();
  return files.map((f) => ({
    input: f.path,
    output: buildOutputPath(
      f.path,
      resolveOutputDir(f.path, settings),
      format(f),
      settings.outputTemplate,
      stamp,
    ),
    duration: f.duration ?? null,
    resolution: f.resolution ?? null,
  }));
}

function queueKey(queued: QueuedFile[]): string {
  return queued.map((f) => f.id).join(" ");
}

/** True if any of `submitted` has reached a terminal state (or was removed). */
function madeProgress(submitted: QueuedFile[]): boolean {
  const current = new Map(store.getState().files.map((f) => [f.id, f.status]));
  return submitted.some((f) => {
    const status = current.get(f.id);
    return status === undefined || status === "complete" || status === "error";
  });
}

/** The compress-tab batch for one media type, using the options current at call time. */
function compressBatchFor(type: MediaType, files: QueuedFile[]): BatchRun {
  const { videoOptions, imageOptions, pdfOptions, audioCompressionOptions } = store.getState();
  switch (type) {
    case "video":
      return {
        files,
        entries: entriesFor(files, () => undefined),
        invoke: (entries, ch) => compressVideosBatch(entries, videoOptions, ch),
      };
    case "image":
      return {
        files,
        entries: entriesFor(files, (f) => resolveImageOutputFormat(imageOptions.format, f.path)),
        invoke: (entries, ch) => compressImagesBatch(entries, imageOptions, ch),
      };
    case "pdf":
      return {
        files,
        entries: entriesFor(files, () => "pdf"),
        invoke: (entries, ch) => compressPdfsBatch(entries, pdfOptions, ch),
      };
    case "audio":
      return {
        files,
        entries: entriesFor(files, (f) =>
          resolveAudioCompressionExtension(audioCompressionOptions.format, f.path),
        ),
        invoke: (entries, ch) => compressAudioBatch(entries, audioCompressionOptions, ch),
      };
  }
}

/**
 * Keep at most one batch per media type in flight until the queue is empty. Each
 * media type is refilled as soon as *its own* batch returns, and files added
 * mid-run start right away if their type is idle, so they never wait on a slower
 * type (e.g. photos dropped in during a long video encode).
 *
 * Resolves once nothing is in flight and nothing more can start: the queue is
 * drained or stalled, or Cancel All was requested. While paused, in-flight batches
 * finish but nothing new starts until Resume.
 */
function drainQueue(): Promise<void> {
  return new Promise<void>((resolve) => {
    const running = new Set<MediaType>();
    const lastSubmitted = new Map<MediaType, string>();
    let finished = false;
    let unsubscribe = () => {};

    const pump = () => {
      if (finished) return;
      const state = store.getState();

      if (!state._cancelRequested && !state.isPaused) {
        const queued = state.files.filter((f) => f.status === "queued");
        for (const type of MEDIA_TYPES) {
          if (running.has(type)) continue;
          const files = sortQueuedFilesForCompression(queued, type);
          if (files.length === 0) continue;

          // Guard against a backend that returns without moving any file to a
          // terminal state: re-submitting the identical queue would spin forever.
          // Cleared whenever a batch makes progress, so a retried file can rerun.
          const key = queueKey(files);
          if (key === lastSubmitted.get(type)) {
            console.warn(`The ${type} queue did not advance; not resubmitting it`);
            continue;
          }
          lastSubmitted.set(type, key);

          running.add(type);
          void runBatch(compressBatchFor(type, files)).finally(() => {
            running.delete(type);
            if (madeProgress(files)) lastSubmitted.delete(type);
            pump();
          });
        }
      }

      if (running.size === 0 && (state._cancelRequested || !state.isPaused)) {
        finished = true;
        unsubscribe();
        resolve();
      }
    };

    // Files added or retried, pause/resume, and Cancel All can all change what
    // should run next.
    unsubscribe = store.subscribe((state, prev) => {
      if (
        state.filesRevision !== prev.filesRevision ||
        state.summary.queued > prev.summary.queued ||
        state.isPaused !== prev.isPaused ||
        state._cancelRequested !== prev._cancelRequested
      ) {
        pump();
      }
    });
    pump();
  });
}

export async function startCompression(): Promise<void> {
  const { outputDir, outputMode, summary } = store.getState();
  if (summary.queued === 0) return;

  if (outputMode === "customDir" && !outputDir) {
    console.error("No output directory selected");
    return;
  }

  store.getState().resetQueueControlFlags();
  try {
    await resetCancel();
  } catch (err) {
    console.warn("resetCancel failed (continuing):", err);
  }

  store.getState().startOperation();
  try {
    await drainQueue();
  } finally {
    store.getState().endOperation();
  }
}

export async function cancelFile(fileId: string): Promise<void> {
  const file = store.getState().files.find((f) => f.id === fileId);
  if (!file) return;

  if (file.jobId && file.status === "processing") {
    try {
      await cancelCompression(file.jobId);
    } catch {
      // Process may have already finished
    }
    store.getState().markError(file.jobId, "Cancelled by user");
  }
}

/** Cancel every file currently processing, leaving the rest of the queue to run. */
export async function cancelProcessingFiles(): Promise<void> {
  const processing = store.getState().files.filter((f) => f.status === "processing");
  await Promise.all(processing.map((f) => cancelFile(f.id)));
}

async function runSingleTool(
  file: QueuedFile,
  ext: string,
  invoke: (output: string, channel: Channel<ProgressEvent>) => Promise<CompressionResult>,
): Promise<void> {
  const settings = outputSettings();
  if (settings.outputMode === "customDir" && !settings.outputDir) {
    console.error("No output directory selected");
    return;
  }
  const output = buildOutputPath(
    file.path,
    resolveOutputDir(file.path, settings),
    ext,
    settings.outputTemplate,
  );

  store.getState().startOperation();
  store.getState().setFileStatus(file.id, "processing", undefined);
  const channel = createProgressChannel([file]);

  try {
    await invoke(output, channel);
  } catch (err) {
    // Don't override a "queued" status — that means cancelAllCompression
    // already reset the file and we should respect that.
    store.getState().markErrorByIds([file.id], String(err), ["processing"]);
  } finally {
    store.getState().endOperation();
  }
}

export async function extractAudioFromFile(file: QueuedFile): Promise<void> {
  const { audioOptions } = store.getState();
  await runSingleTool(file, getAudioExtension(audioOptions.format), (output, ch) =>
    extractAudio(file.path, output, audioOptions, ch, file.duration),
  );
}

export async function convertToGif(file: QueuedFile): Promise<void> {
  const { gifOptions } = store.getState();
  await runSingleTool(file, "gif", (output, ch) =>
    convertVideoToGif(file.path, output, gifOptions, ch, file.duration, file.resolution),
  );
}

async function runToolBatch(
  ext: string,
  invoke: (entries: BatchEntry[], channel: Channel<ProgressEvent>) => Promise<CompressionResult[]>,
): Promise<void> {
  const videoFiles = store
    .getState()
    .files.filter((f) => f.mediaType === "video" && f.status === "queued");
  if (videoFiles.length === 0) return;

  store.getState().startOperation();
  try {
    await runBatch({ files: videoFiles, entries: entriesFor(videoFiles, () => ext), invoke });
  } finally {
    store.getState().endOperation();
  }
}

export async function extractAudioFromAll(): Promise<void> {
  const { audioOptions } = store.getState();
  await runToolBatch(getAudioExtension(audioOptions.format), (entries, ch) =>
    extractAudioBatch(entries, audioOptions, ch),
  );
}

export async function convertAllToGif(): Promise<void> {
  const { gifOptions } = store.getState();
  await runToolBatch("gif", (entries, ch) => convertVideosToGifBatch(entries, gifOptions, ch));
}

export function pauseCompression(): void {
  store.getState().pauseCompression();
}

export function resumeCompression(): void {
  store.getState().resumeCompression();
}

// Stops the queue immediately: kill child processes + raise the cancel flag in
// Rust, then reset the store. Order matters — we set the cancel flag in the
// store *after* the backend kill so the drain loop, when it next checks, sees
// that everything has been torn down.
export async function cancelAllCompression(): Promise<void> {
  try {
    await cancelAllBackend();
  } catch (err) {
    console.warn("cancel_all backend call failed:", err);
  }
  store.getState().cancelAllCompression();
}

export const compressionController = {
  startCompression,
  cancelFile,
  cancelProcessingFiles,
  extractAudioFromFile,
  convertToGif,
  extractAudioFromAll,
  convertAllToGif,
  pauseCompression,
  resumeCompression,
  cancelAllCompression,
};

export type CompressionController = typeof compressionController;
