import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  VideoOptions,
  ImageOptions,
  CompressionResult,
  FileInfo,
  MediaType,
  ProgressEvent,
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
): Promise<CompressionResult> {
  return invoke("compress_image", { input, output, options });
}

export async function compressImagesBatch(
  files: BatchEntry[],
  options: ImageOptions,
): Promise<CompressionResult[]> {
  return invoke("compress_images_batch", { files, options });
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
