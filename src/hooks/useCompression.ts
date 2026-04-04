import { useCallback } from "react";
import { Channel } from "@tauri-apps/api/core";
import {
  compressVideosBatch,
  compressImagesBatch,
  compressPdfsBatch,
  extractAudio,
  convertVideoToGif,
  cancelCompression,
} from "../lib/commands";
import { useCompressionStore } from "../stores/compressionStore";
import { buildOutputPath, getParentDir, getAudioExtension } from "../lib/fileUtils";
import type { ProgressEvent, QueuedFile } from "../types/compression";

export function useCompression() {
  const setFileStatus = useCompressionStore((s) => s.setFileStatus);
  const updateProgress = useCompressionStore((s) => s.updateProgress);
  const markComplete = useCompressionStore((s) => s.markComplete);
  const markError = useCompressionStore((s) => s.markError);
  const setIsCompressing = useCompressionStore((s) => s.setIsCompressing);

  const startCompression = useCallback(async () => {
    const { outputDir, outputMode } = useCompressionStore.getState();

    const initialQueued = useCompressionStore.getState().files.filter((f) => f.status === "queued");
    if (initialQueued.length === 0) return;

    if (outputMode === "customDir" && !outputDir) {
      console.error("No output directory selected");
      return;
    }

    setIsCompressing(true);

    try {
      // Drain-loop: keep processing until no queued files remain.
      // Files added by the user during compression are picked up in the next iteration.
      while (true) {
        const {
          files,
          videoOptions,
          imageOptions,
          pdfOptions,
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

        // --- PDF batch (sequential, indeterminate progress) ---
        if (pdfFiles.length > 0) {
          promises.push(
            (async () => {
              const pdfBatch = pdfFiles.map((f) => ({
                input: f.path,
                output: buildOutputPath(f.path, getOutputDirForFile(f), "pdf", outputTemplate),
              }));

              const pdfFileIds = pdfFiles.map((f) => f.id);
              let pdfIndex = 0;

              const channel = new Channel<ProgressEvent>();
              channel.onmessage = (event: ProgressEvent) => {
                switch (event.event) {
                  case "started": {
                    const fileId = pdfFileIds[pdfIndex];
                    if (fileId) {
                      setFileStatus(fileId, "processing", event.data.jobId);
                    }
                    pdfIndex++;
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
                for (let i = pdfIndex; i < pdfFileIds.length; i++) {
                  const fid = pdfFileIds[i];
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

        await Promise.allSettled(promises);
      }
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

      setIsCompressing(true);
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
        useCompressionStore.setState((state) => ({
          files: state.files.map((f) =>
            f.id === file.id
              ? { ...f, status: "error" as const, error: String(err) }
              : f,
          ),
        }));
      } finally {
        setIsCompressing(false);
      }
    },
    [setFileStatus, updateProgress, markComplete, markError, setIsCompressing],
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

      setIsCompressing(true);
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
        useCompressionStore.setState((state) => ({
          files: state.files.map((f) =>
            f.id === file.id
              ? { ...f, status: "error" as const, error: String(err) }
              : f,
          ),
        }));
      } finally {
        setIsCompressing(false);
      }
    },
    [setFileStatus, updateProgress, markComplete, markError, setIsCompressing],
  );

  return { startCompression, cancelFile, extractAudioFromFile, convertToGif };
}
