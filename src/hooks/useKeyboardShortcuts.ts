import { useEffect } from "react";
import { useCompressionStore } from "../stores/compressionStore";
import { useCompression } from "./useCompression";

/**
 * Global keyboard shortcuts:
 * - Space: start compression (when files are queued and not already compressing)
 * - Escape: cancel all processing files
 */
export function useKeyboardShortcuts() {
  const { startCompression, cancelFile } = useCompression();

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      // Don't intercept shortcuts when typing in inputs
      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable
      ) {
        return;
      }

      if (e.code === "Space") {
        const { files, isCompressing } = useCompressionStore.getState();
        const hasQueued = files.some((f) => f.status === "queued");
        if (hasQueued && !isCompressing) {
          e.preventDefault();
          startCompression();
        }
      }

      if (e.code === "Escape") {
        const { files } = useCompressionStore.getState();
        const processing = files.filter((f) => f.status === "processing");
        for (const file of processing) {
          cancelFile(file.id);
        }
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [startCompression, cancelFile]);
}
