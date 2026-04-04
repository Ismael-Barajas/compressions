import { useCallback } from "react";
import { Channel } from "@tauri-apps/api/core";
import {
  compressVideosBatch,
  compressImagesBatch,
  cancelCompression,
} from "../lib/commands";
import { useCompressionStore } from "../stores/compressionStore";
import { buildOutputPath, getParentDir } from "../lib/fileUtils";
import type { ProgressEvent, QueuedFile } from "../types/compression";

export function useCompression() {
  const setFileStatus = useCompressionStore((s) => s.setFileStatus);
  const updateProgress = useCompressionStore((s) => s.updateProgress);
  const markComplete = useCompressionStore((s) => s.markComplete);
  const markError = useCompressionStore((s) => s.markError);
  const setIsCompressing = useCompressionStore((s) => s.setIsCompressing);

  const startCompression = useCallback(async () => {
    const {
      files,
      videoOptions,
      imageOptions,
      outputDir,
      outputMode,
      subfolderName,
      outputTemplate,
    } = useCompressionStore.getState();

    const queued = files.filter((f) => f.status === "queued");
    if (queued.length === 0) return;

    if (outputMode === "customDir" && !outputDir) {
      console.error("No output directory selected");
      return;
    }

    setIsCompressing(true);

    try {
      const videoFiles = queued.filter((f) => f.mediaType === "video");
      const imageFiles = queued.filter((f) => f.mediaType === "image");

      const getOutputDirForFile = (file: QueuedFile) => {
        const parentDir = getParentDir(file.path);
        const sep = parentDir.includes("\\") ? "\\" : "/";
        switch (outputMode) {
          case "subfolder":
            return `${parentDir}${sep}${subfolderName}`;
          case "customDir":
            return outputDir!;
          default:
            return parentDir;
        }
      };

      const promises: Promise<void>[] = [];

      // --- Video batch (sequential, with progress channel) ---
      if (videoFiles.length > 0) {
        promises.push(
          (async () => {
            const videoBatch = videoFiles.map((f) => ({
              input: f.path,
              output: buildOutputPath(f.path, getOutputDirForFile(f), undefined, outputTemplate),
            }));

            const videoFileIds = videoFiles.map((f) => f.id);
            let videoIndex = 0;

            const channel = new Channel<ProgressEvent>();
            channel.onmessage = (event: ProgressEvent) => {
              switch (event.event) {
                case "started": {
                  const fileId = videoFileIds[videoIndex];
                  if (fileId) {
                    setFileStatus(fileId, "processing", event.data.jobId);
                  }
                  videoIndex++;
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

            try {
              await compressVideosBatch(videoBatch, videoOptions, channel);
            } catch (err) {
              // Mark remaining unstarted video files as error
              for (let i = videoIndex; i < videoFileIds.length; i++) {
                const fid = videoFileIds[i];
                useCompressionStore.setState((state) => ({
                  files: state.files.map((f) =>
                    f.id === fid
                      ? { ...f, status: "error" as const, error: String(err) }
                      : f,
                  ),
                }));
              }
            }
          })(),
        );
      }

      // --- Image batch (parallel, with progress channel) ---
      if (imageFiles.length > 0) {
        promises.push(
          (async () => {
            const imageFormat = imageOptions.format.toLowerCase();
            const imageBatch = imageFiles.map((f) => ({
              input: f.path,
              output: buildOutputPath(f.path, getOutputDirForFile(f), imageFormat, outputTemplate),
            }));

            const channel = new Channel<ProgressEvent>();
            channel.onmessage = (event: ProgressEvent) => {
              switch (event.event) {
                case "started": {
                  // Match the started event to a file by filename
                  const matchedFile = imageFiles.find((f) => f.name === event.data.fileName);
                  if (matchedFile) {
                    setFileStatus(matchedFile.id, "processing", event.data.jobId);
                  }
                  break;
                }
                case "completed":
                  markComplete(event.data.jobId, event.data);
                  break;
                case "error":
                  markError(event.data.jobId, event.data.message);
                  break;
              }
            };

            try {
              await compressImagesBatch(imageBatch, imageOptions, channel);
            } catch (err) {
              for (const f of imageFiles) {
                const current = useCompressionStore
                  .getState()
                  .files.find((sf) => sf.id === f.id);
                if (current && current.status === "processing") {
                  useCompressionStore.setState((state) => ({
                    files: state.files.map((sf) =>
                      sf.id === f.id
                        ? { ...sf, status: "error" as const, error: String(err) }
                        : sf,
                    ),
                  }));
                }
              }
            }
          })(),
        );
      }

      await Promise.allSettled(promises);
    } finally {
      setIsCompressing(false);
    }
  }, [setFileStatus, updateProgress, markComplete, markError, setIsCompressing]);

  const cancelFile = useCallback(
    async (fileId: string) => {
      const file = useCompressionStore
        .getState()
        .files.find((f) => f.id === fileId);
      if (!file) return;

      if (file.jobId && file.status === "processing") {
        try {
          await cancelCompression(file.jobId);
        } catch {
          // Process may have already finished
        }
        markError(file.jobId, "Cancelled by user");
      }
    },
    [markError],
  );

  return { startCompression, cancelFile };
}
