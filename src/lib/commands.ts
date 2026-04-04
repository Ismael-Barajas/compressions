import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  VideoOptions,
  ImageOptions,
  AudioExtractionOptions,
  GifConversionOptions,
  PdfOptions,
  CompressionResult,
  FileInfo,
  MediaType,
  ProgressEvent,
  HistoryEntry,
} from "../types/compression";
import type { Preset } from "../types/presets";

interface BatchEntry {
  input: string;
  output: string;
}

export async function compressVideo(
  input: string,
  output: string,
  options: VideoOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult> {
  return invoke("compress_video", { input, output, options, onProgress });
}

export async function compressVideosBatch(
  files: BatchEntry[],
  options: VideoOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult[]> {
  return invoke("compress_videos_batch", { files, options, onProgress });
}

export async function compressImage(
  input: string,
  output: string,
  options: ImageOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult> {
  return invoke("compress_image", { input, output, options, onProgress });
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

export async function probeFile(path: string): Promise<FileInfo> {
  return invoke("probe_file", { path });
}

export async function detectMediaType(path: string): Promise<MediaType> {
  return invoke("detect_media_type", { path });
}

export async function getPresets(): Promise<Preset[]> {
  return invoke("get_presets");
}

export async function savePreset(preset: Preset): Promise<void> {
  return invoke("save_preset", { preset });
}

export async function deletePreset(id: string): Promise<void> {
  return invoke("delete_preset", { id });
}

export async function getDefaultOutputDir(): Promise<string> {
  return invoke("get_default_output_dir");
}

export async function scanPaths(paths: string[]): Promise<string[]> {
  return invoke("scan_paths", { paths });
}

export async function extractAudio(
  input: string,
  output: string,
  options: AudioExtractionOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult> {
  return invoke("extract_audio", { input, output, options, onProgress });
}

export async function extractAudioBatch(
  files: BatchEntry[],
  options: AudioExtractionOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult[]> {
  return invoke("extract_audio_batch", { files, options, onProgress });
}

export async function convertVideoToGif(
  input: string,
  output: string,
  options: GifConversionOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult> {
  return invoke("convert_video_to_gif", { input, output, options, onProgress });
}

export async function convertVideosToGifBatch(
  files: BatchEntry[],
  options: GifConversionOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult[]> {
  return invoke("convert_videos_to_gif_batch", { files, options, onProgress });
}

export async function compressPdf(
  input: string,
  output: string,
  options: PdfOptions,
  onProgress: Channel<ProgressEvent>,
): Promise<CompressionResult> {
  return invoke("compress_pdf", { input, output, options, onProgress });
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
