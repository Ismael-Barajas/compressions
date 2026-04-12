import { useRef, useEffect, useCallback } from "react";
import { Plus, Trash2, Play, FolderOpen, LayoutGrid, LayoutList } from "lucide-react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useCompressionStore } from "../../stores/compressionStore";
import { useCompression } from "../../hooks/useCompression";
import { scanPaths, generateThumbnailsBatch } from "../../lib/commands";
import { pathsToQueuedFiles } from "../../lib/fileUtils";
import { FileItem } from "./FileItem";

function addResolvedPaths(paths: string[]) {
  const newFiles = pathsToQueuedFiles(paths);
  if (newFiles.length > 0) {
    useCompressionStore.getState().addFiles(newFiles);
  }
}

// Row heights for the virtualizer
const COMPACT_ROW_HEIGHT = 62;
const THUMB_ROW_HEIGHT = 96;

export function FileList() {
  const files = useCompressionStore((s) => s.files);
  const clearFiles = useCompressionStore((s) => s.clearFiles);
  const isCompressing = useCompressionStore((s) => s.isCompressing);
  const showThumbnails = useCompressionStore((s) => s.showThumbnails);
  const toggleThumbnails = useCompressionStore((s) => s.toggleThumbnails);
  const setThumbnailPath = useCompressionStore((s) => s.setThumbnailPath);
  const { startCompression } = useCompression();

  const scrollRef = useRef<HTMLDivElement>(null);
  const inFlightRef = useRef(new Set<string>());
  const failedRef = useRef(new Set<string>());

  const useVirtual = showThumbnails || files.length > 200;
  const rowHeight = showThumbnails ? THUMB_ROW_HEIGHT : COMPACT_ROW_HEIGHT;

  const virtualizer = useVirtualizer({
    count: files.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 5,
    enabled: useVirtual,
  });

  // Viewport-aware thumbnail generation.
  // Reads files/showThumbnails from the store snapshot to avoid capturing them
  // as deps — otherwise every setThumbnailPath call would recreate this callback
  // and refire the effects below.
  const generateVisibleThumbnails = useCallback(() => {
    const currentFiles = useCompressionStore.getState().files;
    const currentShow = useCompressionStore.getState().showThumbnails;
    if (!currentShow || currentFiles.length === 0) return;

    const visibleItems = virtualizer.getVirtualItems();
    const needThumbnails: { id: string; path: string }[] = [];

    for (const item of visibleItems) {
      const file = currentFiles[item.index];
      if (
        file &&
        !file.thumbnailPath &&
        !inFlightRef.current.has(file.id) &&
        !failedRef.current.has(file.id) &&
        file.mediaType !== "pdf"
      ) {
        needThumbnails.push({ id: file.id, path: file.path });
        inFlightRef.current.add(file.id);
      }
    }

    if (needThumbnails.length === 0) return;

    const paths = needThumbnails.map((f) => f.path);
    generateThumbnailsBatch(paths)
      .then((results) => {
        for (const [path, thumbPath] of results) {
          const match = needThumbnails.find((f) => f.path === path);
          if (match) {
            inFlightRef.current.delete(match.id);
            if (thumbPath) {
              setThumbnailPath(match.id, thumbPath);
            } else {
              failedRef.current.add(match.id);
            }
          }
        }
      })
      .catch(() => {
        for (const f of needThumbnails) {
          inFlightRef.current.delete(f.id);
        }
      });
  }, [virtualizer, setThumbnailPath]);

  // Generate thumbnails when visible range changes or thumbnails toggled on
  useEffect(() => {
    if (!showThumbnails) return;
    const timer = setTimeout(generateVisibleThumbnails, 100);
    return () => clearTimeout(timer);
  }, [showThumbnails, generateVisibleThumbnails, virtualizer.getVirtualItems().length]);

  // Also generate when files are first added with thumbnails already on
  useEffect(() => {
    if (!showThumbnails || files.length === 0) return;
    const timer = setTimeout(generateVisibleThumbnails, 200);
    return () => clearTimeout(timer);
  }, [files.length, showThumbnails, generateVisibleThumbnails]);

  // Clear in-flight tracking when thumbnails toggled off
  useEffect(() => {
    if (!showThumbnails) {
      inFlightRef.current.clear();
      failedRef.current.clear();
    }
  }, [showThumbnails]);

  const handleAddMore = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: true,
        filters: [
          {
            name: "Media Files",
            extensions: [
              "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v",
              "jpg", "jpeg", "png", "webp", "avif", "bmp", "tiff", "gif",
              "pdf",
            ],
          },
        ],
      });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        const resolved = await scanPaths(paths);
        addResolvedPaths(resolved);
      }
    } catch {
      // cancelled
    }
  };

  const handleAddFolder = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true });
      if (selected) {
        const resolved = await scanPaths([selected as string]);
        addResolvedPaths(resolved);
      }
    } catch {
      // cancelled
    }
  };

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* Toolbar */}
      <div
        className="mb-3 flex items-center justify-between border-b pb-3"
        style={{ borderColor: "var(--border)" }}
      >
        <span className="font-data" style={{ color: "var(--text-muted)" }}>
          {files.length} file{files.length !== 1 ? "s" : ""} queued
        </span>
        <div className="flex gap-1.5">
          <ToolbarButton onClick={toggleThumbnails} title={showThumbnails ? "List view" : "Thumbnail view"}>
            {showThumbnails ? <LayoutList size={13} /> : <LayoutGrid size={13} />}
          </ToolbarButton>
          <ToolbarButton onClick={handleAddMore}>
            <Plus size={13} />
            Add
          </ToolbarButton>
          <ToolbarButton onClick={handleAddFolder}>
            <FolderOpen size={13} />
            Folder
          </ToolbarButton>
          <ToolbarButton onClick={clearFiles} danger>
            <Trash2 size={13} />
            Clear
          </ToolbarButton>
        </div>
      </div>

      {/* File list */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto">
        {useVirtual ? (
          <div
            style={{
              height: virtualizer.getTotalSize(),
              width: "100%",
              position: "relative",
            }}
          >
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const file = files[virtualRow.index];
              return (
                <div
                  key={file.id}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    height: virtualRow.size,
                    transform: `translateY(${virtualRow.start}px)`,
                  }}
                >
                  <div className="pb-1.5">
                    <FileItem file={file} showThumbnails={showThumbnails} />
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="space-y-1.5">
            {files.map((file) => (
              <FileItem key={file.id} file={file} showThumbnails={showThumbnails} />
            ))}
          </div>
        )}
      </div>

      {/* Compress button */}
      {!isCompressing && files.some((f) => f.status === "queued") && (
        <div className="mt-4 flex justify-center">
          <button
            className="btn-primary flex items-center gap-2.5 px-10 py-2.5 text-[15px]"
            onClick={startCompression}
          >
            <Play size={17} fill="currentColor" />
            Compress
          </button>
        </div>
      )}
    </div>
  );
}

function ToolbarButton({
  children,
  danger,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { danger?: boolean }) {
  return (
    <button
      className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium transition-colors duration-100"
      style={{
        color: danger ? "var(--error)" : "var(--text-secondary)",
        border: "1px solid var(--border)",
        backgroundColor: "transparent",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.borderColor = danger ? "var(--error)" : "var(--border-hover)";
        e.currentTarget.style.backgroundColor = "var(--bg-tertiary)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.borderColor = "var(--border)";
        e.currentTarget.style.backgroundColor = "transparent";
      }}
      {...props}
    >
      {children}
    </button>
  );
}
