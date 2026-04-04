export type MediaType = "video" | "image" | "pdf";

export type VideoCodec = "H264" | "H265" | "AV1";
export type AudioCodec = "AAC" | "Opus" | "Copy" | "None";

export type ImageFormat = "Jpeg" | "Png" | "WebP" | "Avif" | "Gif";

export type AudioOutputFormat = "Mp3" | "Aac" | "Flac" | "Opus" | "Wav";

export interface AudioExtractionOptions {
  format: AudioOutputFormat;
  bitrate: string | null;
  sampleRate: number | null;
}

export type PdfQuality = "screen" | "ebook" | "printer" | "prepress";

export interface PdfOptions {
  quality: PdfQuality;
  dpi: number | null;
}

export type DitherMode = "bayer" | "floyd_steinberg" | "none";

export interface GifConversionOptions {
  fps: number;
  width: number | null;
  maxColors: number;
  dither: DitherMode;
}

export type FileStatus = "queued" | "processing" | "complete" | "error";

export type OutputMode = "sameDir" | "subfolder" | "customDir";

export interface Resolution {
  width: number;
  height: number;
}

export interface VideoOptions {
  codec: VideoCodec;
  crf: number;
  resolution: Resolution | null;
  bitrate: string | null;
  framerate: number | null;
  audioCodec: AudioCodec;
  audioBitrate: string | null;
}

export interface ImageOptions {
  format: ImageFormat;
  quality: number;
  resize: Resolution | null;
  stripMetadata: boolean;
}

export interface ProgressPayload {
  jobId: string;
  fileName: string;
  percent: number;
  currentFrame: number | null;
  totalFrames: number | null;
  speed: string | null;
  etaSeconds: number | null;
}

export interface CompressionResult {
  jobId: string;
  inputPath: string;
  outputPath: string;
  inputSize: number;
  outputSize: number;
  durationMs: number;
  success: boolean;
  error: string | null;
}

export interface FileInfo {
  path: string;
  fileName: string;
  size: number;
  mediaType: MediaType;
  duration: number | null;
  resolution: Resolution | null;
  codecName: string | null;
}

export interface QueuedFile {
  id: string;
  path: string;
  name: string;
  size: number;
  mediaType: MediaType;
  status: FileStatus;
  progress: number;
  resolution?: Resolution | null;
  duration?: number | null;
  jobId?: string;
  result?: CompressionResult;
  error?: string;
}

export interface HistoryEntry {
  id: string;
  timestamp: string;
  inputPath: string;
  outputPath: string;
  inputSize: number;
  outputSize: number;
  durationMs: number;
  mediaType: string;
  success: boolean;
  error: string | null;
}

export type ProgressEvent =
  | { event: "started"; data: { jobId: string; fileName: string } }
  | { event: "progress"; data: ProgressPayload }
  | { event: "completed"; data: CompressionResult }
  | { event: "error"; data: { jobId: string; message: string } };
