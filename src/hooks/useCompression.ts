import { useCallback } from "react";
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
} from "../lib/commands";
import { useCompressionStore } from "../stores/compressionStore";
import { buildOutputPath, getParentDir, getAudioExtension, resolveAudioCompressionExtension } from "../lib/fileUtils";
import type { ProgressEvent, QueuedFile } from "../types/compression";

// Resolves once the store's isPaused flips back to false OR a cancel is requested.
// Uses zustand subscribe (no polling) and immediately re-checks state to close the
// race window between the caller reading state and us subscribing.
function waitWhilePaused(): Promise<void> {
  return new Promise<void>((resolve) => {
    const check = () => {
      const s = useCompressionStore.getState();
      return !s.isPaused || s._cancelRequested;
    };
    if (check()) {
      resolve();
      return;
    }
    const unsub = useCompressionStore.subscribe((state) => {
      if (!state.isPaused || state._cancelRequested) {
        unsub();
        resolve();
      }
    });
  });
}

export function useCompression() {
  const setFileStatus = useCompressionStore((s) => s.setFileStatus);
  const updateProgress = useCompressionStore((s) => s.updateProgress);
  const markComplete = useCompressionStore((s) => s.markComplete);
  const markError = useCompressionStore((s) => s.markError);
  const startOperation = useCompressionStore((s) => s.startOperation);
  const endOperation = useCompressionStore((s) => s.endOperation);

  const startCompression = useCallback(async () => {
    const { outputDir, outputMode } = useCompressionStore.getState();

    const initialQueued = useCompressionStore.getState().files.filter((f) => f.status === "queued");
    if (initialQueued.length === 0) return;

    if (outputMode === "customDir" && !outputDir) {
      console.error("No output directory selected");
      return;
    }

    useCompressionStore.getState().resetQueueControlFlags();
    try {
      await resetCancel();
    } catch (err) {
      console.warn("resetCancel failed (continuing):", err);
    }

    startOperation();

    try {
      // Drain-loop: keep processing until no queued files remain.
      // Files added by the user during compression are picked up in the next iteration.
      while (true) {
        // Bail-out point honored every iteration. cancelAll() sets _cancelRequested
        // *and* kills child processes, so by the time we re-enter here the in-flight
        // batches will have errored out and the store state has been reset.
        if (useCompressionStore.getState()._cancelRequested) break;

        // Soft pause: park the drain until the user resumes (or cancels).
        if (useCompressionStore.getState().isPaused) {
          await waitWhilePaused();
          if (useCompressionStore.getState()._cancelRequested) break;
        }

        const {
          files,
          videoOptions,
          imageOptions,
          pdfOptions,
          audioCompressionOptions,
          outputDir: currentOutputDir,
          outputMode: currentOutputMode,
          subfolderName,
          outputTemplate,
        } = useCompressionStore.getState();

        const queued = files.filter((f) => f.status === "queued");
        if (queued.length === 0) break;

        const getOutputDirForFile = (file: QueuedFile) => {
          const parentDir = getParentDir(file.path);
          const sep = parentDir.includes("\\") ? "\\" : "/";
          switch (currentOutputMode) {
            case "subfolder":
              return `${parentDir}${sep}${subfolderName}`;
            case "customDir":
              return currentOutputDir!;
            default:
              return parentDir;
          }
        };

        const videoFiles = queued.filter((f) => f.mediaType === "video");
        const imageFiles = queued.filter((f) => f.mediaType === "image");
        const pdfFiles = queued.filter((f) => f.mediaType === "pdf");
        const audioFiles = queued.filter((f) => f.mediaType === "audio");

        const promises: Promise<void>[] = [];

        // --- Video batch (sequential, with progress channel) ---
        if (videoFiles.length > 0) {
          promises.push(
            (async () => {
              const videoBatch = videoFiles.map((f) => ({
                input: f.path,
                output: buildOutputPath(f.path, getOutputDirForFile(f), undefined, outputTemplate),
              }));

              const channel = new Channel<ProgressEvent>();
              channel.onmessage = (event: ProgressEvent) => {
                switch (event.event) {
                  case "started": {
                    // Match by inputPath rather than insertion order — robust to
                    // any future case where the backend skips a file before sending Started.
                    const matched = videoFiles.find((f) => f.path === event.data.inputPath);
                    if (matched) {
                      setFileStatus(matched.id, "processing", event.data.jobId);
                    }
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
                // Mark any file still queued (never started) as error.
                for (const f of videoFiles) {
                  const current = useCompressionStore
                    .getState()
                    .files.find((sf) => sf.id === f.id);
                  if (current && current.status === "queued") {
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

        // --- Image batch (parallel, with progress channel) ---
        if (imageFiles.length > 0) {
          promises.push(
            (async () => {
              const imageBatch = imageFiles.map((f) => {
                let format: string | undefined;
                if (imageOptions.format === "Original") {
                  const ext = f.path.slice(f.path.lastIndexOf(".")).toLowerCase();
                  const keepExt = [".jpg", ".jpeg", ".png", ".webp", ".avif", ".gif"];
                  format = keepExt.includes(ext) ? undefined : "png";
                } else {
                  format = imageOptions.format.toLowerCase();
                }
                return {
                  input: f.path,
                  output: buildOutputPath(f.path, getOutputDirForFile(f), format, outputTemplate),
                };
              });

              const channel = new Channel<ProgressEvent>();
              channel.onmessage = (event: ProgressEvent) => {
                switch (event.event) {
                  case "started": {
                    const matchedFile = imageFiles.find((f) => f.path === event.data.inputPath);
                    if (matchedFile) {
                      setFileStatus(matchedFile.id, "processing", event.data.jobId);
                    }
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

        // --- PDF batch (sequential, indeterminate progress) ---
        if (pdfFiles.length > 0) {
          promises.push(
            (async () => {
              const pdfBatch = pdfFiles.map((f) => ({
                input: f.path,
                output: buildOutputPath(f.path, getOutputDirForFile(f), "pdf", outputTemplate),
              }));

              const channel = new Channel<ProgressEvent>();
              channel.onmessage = (event: ProgressEvent) => {
                switch (event.event) {
                  case "started": {
                    const matched = pdfFiles.find((f) => f.path === event.data.inputPath);
                    if (matched) {
                      setFileStatus(matched.id, "processing", event.data.jobId);
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
                await compressPdfsBatch(pdfBatch, pdfOptions, channel);
              } catch (err) {
                for (const f of pdfFiles) {
                  const current = useCompressionStore
                    .getState()
                    .files.find((sf) => sf.id === f.id);
                  if (current && current.status === "queued") {
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

        // --- Audio batch (sequential, with progress channel) ---
        if (audioFiles.length > 0) {
          promises.push(
            (async () => {
              const audioBatch = audioFiles.map((f) => ({
                input: f.path,
                output: buildOutputPath(
                  f.path,
                  getOutputDirForFile(f),
                  resolveAudioCompressionExtension(audioCompressionOptions.format, f.path),
                  outputTemplate,
                ),
              }));

              const channel = new Channel<ProgressEvent>();
              channel.onmessage = (event: ProgressEvent) => {
                switch (event.event) {
                  case "started": {
                    const matched = audioFiles.find((f) => f.path === event.data.inputPath);
                    if (matched) {
                      setFileStatus(matched.id, "processing", event.data.jobId);
                    }
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
                await compressAudioBatch(audioBatch, audioCompressionOptions, channel);
              } catch (err) {
                for (const f of audioFiles) {
                  const current = useCompressionStore
                    .getState()
                    .files.find((sf) => sf.id === f.id);
                  if (current && current.status === "queued") {
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
      }
    } finally {
      endOperation();
    }
  }, [setFileStatus, updateProgress, markComplete, markError, startOperation, endOperation]);

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

  const extractAudioFromFile = useCallback(
    async (file: QueuedFile) => {
      const {
        audioOptions,
        outputDir,
        outputMode,
        subfolderName,
        outputTemplate,
      } = useCompressionStore.getState();

      const parentDir = getParentDir(file.path);
      const sep = parentDir.includes("\\") ? "\\" : "/";
      let outDir: string;
      switch (outputMode) {
        case "subfolder":
          outDir = `${parentDir}${sep}${subfolderName}`;
          break;
        case "customDir":
          if (!outputDir) {
            console.error("No output directory selected");
            return;
          }
          outDir = outputDir;
          break;
        default:
          outDir = parentDir;
      }

      const audioExt = getAudioExtension(audioOptions.format);
      const output = buildOutputPath(file.path, outDir, audioExt, outputTemplate);

      startOperation();
      setFileStatus(file.id, "processing", undefined);

      const channel = new Channel<ProgressEvent>();
      channel.onmessage = (event: ProgressEvent) => {
        switch (event.event) {
          case "started":
            setFileStatus(file.id, "processing", event.data.jobId);
            break;
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
        await extractAudio(file.path, output, audioOptions, channel);
      } catch (err) {
        // Don't override a "queued" status — that means cancelAllCompression
        // already reset the file and we should respect that.
        useCompressionStore.setState((state) => ({
          files: state.files.map((f) =>
            f.id === file.id && f.status === "processing"
              ? { ...f, status: "error" as const, error: String(err) }
              : f,
          ),
        }));
      } finally {
        endOperation();
      }
    },
    [setFileStatus, updateProgress, markComplete, markError, startOperation, endOperation],
  );

  const convertToGif = useCallback(
    async (file: QueuedFile) => {
      const {
        gifOptions,
        outputDir,
        outputMode,
        subfolderName,
        outputTemplate,
      } = useCompressionStore.getState();

      const parentDir = getParentDir(file.path);
      const sep = parentDir.includes("\\") ? "\\" : "/";
      let outDir: string;
      switch (outputMode) {
        case "subfolder":
          outDir = `${parentDir}${sep}${subfolderName}`;
          break;
        case "customDir":
          if (!outputDir) {
            console.error("No output directory selected");
            return;
          }
          outDir = outputDir;
          break;
        default:
          outDir = parentDir;
      }

      const output = buildOutputPath(file.path, outDir, "gif", outputTemplate);

      startOperation();
      setFileStatus(file.id, "processing", undefined);

      const channel = new Channel<ProgressEvent>();
      channel.onmessage = (event: ProgressEvent) => {
        switch (event.event) {
          case "started":
            setFileStatus(file.id, "processing", event.data.jobId);
            break;
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
        await convertVideoToGif(file.path, output, gifOptions, channel);
      } catch (err) {
        // Same guard as extractAudioFromFile: respect a prior cancelAll reset.
        useCompressionStore.setState((state) => ({
          files: state.files.map((f) =>
            f.id === file.id && f.status === "processing"
              ? { ...f, status: "error" as const, error: String(err) }
              : f,
          ),
        }));
      } finally {
        endOperation();
      }
    },
    [setFileStatus, updateProgress, markComplete, markError, startOperation, endOperation],
  );

  const extractAudioFromAll = useCallback(async () => {
    const {
      audioOptions, outputDir, outputMode, subfolderName, outputTemplate, files,
    } = useCompressionStore.getState();

    const videoFiles = files.filter(
      (f) => f.mediaType === "video" && f.status === "queued",
    );
    if (videoFiles.length === 0) return;

    startOperation();

    const audioExt = getAudioExtension(audioOptions.format);
    const batch = videoFiles.map((f) => {
      const parentDir = getParentDir(f.path);
      const sep = parentDir.includes("\\") ? "\\" : "/";
      let outDir: string;
      switch (outputMode) {
        case "subfolder":
          outDir = `${parentDir}${sep}${subfolderName}`;
          break;
        case "customDir":
          outDir = outputDir ?? parentDir;
          break;
        default:
          outDir = parentDir;
      }
      return {
        input: f.path,
        output: buildOutputPath(f.path, outDir, audioExt, outputTemplate),
      };
    });

    const channel = new Channel<ProgressEvent>();
    channel.onmessage = (event: ProgressEvent) => {
      switch (event.event) {
        case "started": {
          const matched = videoFiles.find((f) => f.path === event.data.inputPath);
          if (matched) setFileStatus(matched.id, "processing", event.data.jobId);
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
      await extractAudioBatch(batch, audioOptions, channel);
    } catch (err) {
      for (const f of videoFiles) {
        const current = useCompressionStore.getState().files.find((sf) => sf.id === f.id);
        if (current && current.status === "processing") {
          useCompressionStore.setState((state) => ({
            files: state.files.map((sf) =>
              sf.id === f.id ? { ...sf, status: "error" as const, error: String(err) } : sf,
            ),
          }));
        }
      }
    } finally {
      endOperation();
    }
  }, [setFileStatus, updateProgress, markComplete, markError, startOperation, endOperation]);

  const convertAllToGif = useCallback(async () => {
    const {
      gifOptions, outputDir, outputMode, subfolderName, outputTemplate, files,
    } = useCompressionStore.getState();

    const videoFiles = files.filter(
      (f) => f.mediaType === "video" && f.status === "queued",
    );
    if (videoFiles.length === 0) return;

    startOperation();

    const batch = videoFiles.map((f) => {
      const parentDir = getParentDir(f.path);
      const sep = parentDir.includes("\\") ? "\\" : "/";
      let outDir: string;
      switch (outputMode) {
        case "subfolder":
          outDir = `${parentDir}${sep}${subfolderName}`;
          break;
        case "customDir":
          outDir = outputDir ?? parentDir;
          break;
        default:
          outDir = parentDir;
      }
      return {
        input: f.path,
        output: buildOutputPath(f.path, outDir, "gif", outputTemplate),
      };
    });

    const channel = new Channel<ProgressEvent>();
    channel.onmessage = (event: ProgressEvent) => {
      switch (event.event) {
        case "started": {
          const matched = videoFiles.find((f) => f.path === event.data.inputPath);
          if (matched) setFileStatus(matched.id, "processing", event.data.jobId);
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
      await convertVideosToGifBatch(batch, gifOptions, channel);
    } catch (err) {
      for (const f of videoFiles) {
        const current = useCompressionStore.getState().files.find((sf) => sf.id === f.id);
        if (current && current.status === "processing") {
          useCompressionStore.setState((state) => ({
            files: state.files.map((sf) =>
              sf.id === f.id ? { ...sf, status: "error" as const, error: String(err) } : sf,
            ),
          }));
        }
      }
    } finally {
      endOperation();
    }
  }, [setFileStatus, updateProgress, markComplete, markError, startOperation, endOperation]);

  const pauseCompression = useCallback(() => {
    useCompressionStore.getState().pauseCompression();
  }, []);

  const resumeCompression = useCallback(() => {
    useCompressionStore.getState().resumeCompression();
  }, []);

  // Stops the queue immediately: kill child processes + raise the cancel flag in
  // Rust, then reset the store. Order matters — we set the cancel flag in the
  // store *after* the backend kill so the drain loop, when it next checks, sees
  // that everything has been torn down.
  const cancelAllCompression = useCallback(async () => {
    try {
      await cancelAllBackend();
    } catch (err) {
      console.warn("cancel_all backend call failed:", err);
    }
    useCompressionStore.getState().cancelAllCompression();
  }, []);

  return {
    startCompression,
    cancelFile,
    extractAudioFromFile,
    convertToGif,
    extractAudioFromAll,
    convertAllToGif,
    pauseCompression,
    resumeCompression,
    cancelAllCompression,
  };
}
