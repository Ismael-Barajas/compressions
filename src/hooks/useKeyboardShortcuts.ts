import { useEffect } from "react";
import { useCompressionStore } from "../stores/compressionStore";
import { startCompression, cancelProcessingFiles } from "../lib/compressionController";

/**
 * Global keyboard shortcuts:
 * - Space: start compression (when files are queued and not already compressing)
 * - Escape: cancel all processing files (no-op while a dialog overlay is open;
 *   the overlay owns Escape to close itself)
 */
export function useKeyboardShortcuts() {
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      // A held key fires repeated keydown events; act once per press.
      if (e.repeat) return;

      // Don't intercept shortcuts when typing in inputs or focused on controls
      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT" ||
        target.tagName === "BUTTON" ||
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
        if (document.querySelector('[role="dialog"]')) return;
        cancelProcessingFiles();
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);
}
