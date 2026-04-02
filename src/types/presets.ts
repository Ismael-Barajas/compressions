import type { MediaType, VideoOptions, ImageOptions } from "./compression";

export interface Preset {
  id: string;
  name: string;
  description: string;
  isBuiltin: boolean;
  mediaType: MediaType;
  videoOptions: VideoOptions | null;
  imageOptions: ImageOptions | null;
}
