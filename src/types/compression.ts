export type MediaType = "video" | "image";

export type VideoCodec = "H264" | "H265" | "AV1";
export type AudioCodec = "AAC" | "Opus" | "Copy" | "None";

export type ImageFormat = "Jpeg" | "Png" | "WebP" | "Avif";

export type FileStatus = "queued" | "processing" | "complete" | "error";

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
  jobId?: string;
  result?: CompressionResult;
  error?: string;
}

export type ProgressEvent =
  | { event: "started"; data: { jobId: string; fileName: string } }
  | { event: "progress"; data: ProgressPayload }
  | { event: "completed"; data: CompressionResult }
  | { event: "error"; data: { jobId: string; message: string } };
