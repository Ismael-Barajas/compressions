import { useEffect } from "react";
import { useCompressionStore } from "../stores/compressionStore";
import { startCompression, cancelProcessingFiles } from "../lib/compressionController";

/**
 * Global keyboard shortcuts:
 * - Space: start compression (when files are queued and not already compressing)
 * - Escape: cancel all processing files
 */
export function useKeyboardShortcuts() {
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
        const { summary, isCompressing } = useCompressionStore.getState();
        if (summary.queued > 0 && !isCompressing) {
          e.preventDefault();
          startCompression();
        }
      }

      if (e.code === "Escape") {
        cancelProcessingFiles();
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);
}
