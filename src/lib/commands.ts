import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  VideoOptions,
  ImageOptions,
  AudioExtractionOptions,
  AudioCompressionOptions,
  GifConversionOptions,
  PdfOptions,
  CompressionResult,
  ProgressEvent,
  ProbeEvent,
  HistoryEntry,
  LogEntry,
  SupportedMedia,
} from "../types/compression";

/** One file in a batch. `duration` (seconds) lets the backend skip re-probing. */
export interface BatchEntry {
  input: string;
  output: string;
  duration?: number | null;
}

export async function compressVideosBatch(
  files: BatchEntry[],
  options: VideoOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult[]> {
  return invoke("compress_videos_batch", { files, options, onProgress });
}

export async function compressImagesBatch(
  files: BatchEntry[],
  options: ImageOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult[]> {
  return invoke("compress_images_batch", { files, options, onProgress });
}

export async function cancelCompression(jobId: string): Promise<void> {
  return invoke("cancel_compression", { jobId });
}

export async function cancelAll(): Promise<void> {
  return invoke("cancel_all");
}

export async function resetCancel(): Promise<void> {
  return invoke("reset_cancel");
}

export async function probeFilesBatch(
  paths: string[],
  onResult: (event: ProbeEvent) => void,
): Promise<void> {
  const channel = new Channel<ProbeEvent>();
  channel.onmessage = onResult;
  return invoke("probe_files_batch", { paths, onResult: channel });
}

export async function getDefaultOutputDir(): Promise<string> {
  return invoke("get_default_output_dir");
}

export async function getSupportedMedia(): Promise<SupportedMedia> {
  return invoke("get_supported_media");
}

export async function scanPaths(paths: string[]): Promise<string[]> {
  return invoke("scan_paths", { paths });
}

export async function extractAudio(
  input: string,
  output: string,
  options: AudioExtractionOptions,
  onProgress: Channel<ProgressEvent>,
  duration?: number | null,
): Promise<CompressionResult> {
  return invoke("extract_audio", { input, output, options, duration: duration ?? null, onProgress });
}

export async function extractAudioBatch(
  files: BatchEntry[],
  options: AudioExtractionOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult[]> {
  return invoke("extract_audio_batch", { files, options, onProgress });
}

export async function compressAudioBatch(
  files: BatchEntry[],
  options: AudioCompressionOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult[]> {
  return invoke("compress_audio_batch", { files, options, onProgress });
}

export async function convertVideoToGif(
  input: string,
  output: string,
  options: GifConversionOptions,
  onProgress: Channel<ProgressEvent>,
  duration?: number | null,
): Promise<CompressionResult> {
  return invoke("convert_video_to_gif", { input, output, options, duration: duration ?? null, onProgress });
}

export async function convertVideosToGifBatch(
  files: BatchEntry[],
  options: GifConversionOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult[]> {
  return invoke("convert_videos_to_gif_batch", { files, options, onProgress });
}

export async function compressPdfsBatch(
  files: BatchEntry[],
  options: PdfOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult[]> {
  return invoke("compress_pdfs_batch", { files, options, onProgress });
}

export async function readClipboardFiles(): Promise<string[]> {
  return invoke("read_clipboard_files");
}

export async function saveClipboardImage(): Promise<string> {
  return invoke("save_clipboard_image");
}

export async function getHistory(): Promise<HistoryEntry[]> {
  return invoke("get_history");
}

export async function clearHistory(): Promise<void> {
  return invoke("clear_history");
}

export async function getLogPath(): Promise<string> {
  return invoke("get_log_path");
}

export async function readLogs(maxLines?: number): Promise<LogEntry[]> {
  return invoke("read_logs", { maxLines: maxLines ?? null });
}

export async function clearLogs(): Promise<void> {
  return invoke("clear_logs");
}

export async function generateThumbnailsBatch(paths: string[]): Promise<[string, string | null][]> {
  return invoke("generate_thumbnails_batch", { paths });
}

export async function clearThumbnailCache(): Promise<void> {
  return invoke("clear_thumbnail_cache");
}
