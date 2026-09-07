import { getSupportedMedia } from "./commands";
import type { MediaType, SupportedMedia } from "../types/compression";

/**
 * Supported extensions. The backend is the source of truth (`get_supported_media`);
 * this static copy only covers the window before the first IPC round trip and
 * non-Tauri environments such as tests.
 */
const FALLBACK: SupportedMedia = {
  video: ["mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "ts"],
  image: ["jpg", "jpeg", "png", "webp", "avif", "bmp", "tiff", "tif", "gif", "heic", "heif"],
  audio: [
    "mp3", "aac", "m4a", "flac", "wav", "ogg", "opus", "wma",
    "aiff", "ape", "alac", "ac3", "dts", "pcm", "amr",
  ],
  pdf: ["pdf"],
};

let current: SupportedMedia = FALLBACK;
let typeByExt = buildLookup(current);
let loaded: Promise<SupportedMedia> | null = null;

function buildLookup(media: SupportedMedia): Map<string, MediaType> {
  const map = new Map<string, MediaType>();
  const add = (exts: string[], type: MediaType) => {
    for (const e of exts) map.set(`.${e.toLowerCase()}`, type);
  };
  add(media.video, "video");
  add(media.image, "image");
  add(media.pdf, "pdf");
  add(media.audio, "audio");
  return map;
}

/** Fetch the backend's list once; later calls return the cached promise. */
export function loadSupportedMedia(): Promise<SupportedMedia> {
  if (!loaded) {
    loaded = getSupportedMedia()
      .then((media) => {
        current = media;
        typeByExt = buildLookup(media);
        return media;
      })
      .catch(() => current);
  }
  return loaded;
}

export function getSupportedMediaSync(): SupportedMedia {
  return current;
}

export function mediaTypeForExtension(extWithDot: string): MediaType | null {
  return typeByExt.get(extWithDot.toLowerCase()) ?? null;
}

/** All extensions, for a single "Media Files" dialog filter. */
export function allExtensions(media: SupportedMedia = current): string[] {
  return [...media.video, ...media.image, ...media.audio, ...media.pdf];
}

/** Dialog filters grouped per media type, derived from the backend list. */
export function dialogFilters(media: SupportedMedia = current): { name: string; extensions: string[] }[] {
  return [
    { name: "Media Files", extensions: allExtensions(media) },
    { name: "Video Files", extensions: media.video },
    { name: "Image Files", extensions: media.image },
    { name: "Audio Files", extensions: media.audio },
    { name: "PDF Files", extensions: media.pdf },
  ];
}

/** Test hook: replace the active list without IPC. */
export function _setSupportedMediaForTests(media: SupportedMedia | null): void {
  current = media ?? FALLBACK;
  typeByExt = buildLookup(current);
}
