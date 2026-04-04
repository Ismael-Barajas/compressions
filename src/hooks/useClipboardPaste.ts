import { useEffect, useCallback } from "react";
import { readClipboardFiles, saveClipboardImage } from "../lib/commands";

/**
 * Listens for Ctrl+V / Cmd+V paste events at the document level.
 * Handles two cases:
 * 1. Clipboard contains an image → saves to temp PNG via Rust, returns path
 * 2. Clipboard contains file paths (text) → reads via Rust, returns paths
 */
export function useClipboardPaste(onFiles: (paths: string[]) => void) {
  const handlePaste = useCallback(
    async (e: ClipboardEvent) => {
      // Don't intercept paste in input/textarea elements
      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable
      ) {
        return;
      }

      e.preventDefault();

      // Try clipboard image first (screenshot / copied image)
      try {
        const imagePath = await saveClipboardImage();
        if (imagePath) {
          onFiles([imagePath]);
          return;
        }
      } catch {
        // No image in clipboard — fall through to file paths
      }

      // Try file paths from clipboard
      try {
        const paths = await readClipboardFiles();
        if (paths.length > 0) {
          onFiles(paths);
        }
      } catch {
        // Nothing usable in clipboard
      }
    },
    [onFiles],
  );

  useEffect(() => {
    document.addEventListener("paste", handlePaste);
    return () => document.removeEventListener("paste", handlePaste);
  }, [handlePaste]);
}
