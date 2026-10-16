import { lazy, Suspense, useRef, useEffect, useCallback } from "react";
import { Plus, Trash2, Play, Pause, Square, FolderOpen, LayoutGrid, LayoutList, List, Workflow } from "lucide-react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useCompressionStore, type FileView } from "../../stores/compressionStore";
import { useCompression } from "../../hooks/useCompression";
import { generateThumbnailsBatch } from "../../lib/commands";
import { scanForQueue } from "../../lib/scanForQueue";
import { pathsToQueuedFiles } from "../../lib/fileUtils";
import { dialogFilters } from "../../lib/mediaTypes";
import { FileItem } from "./FileItem";

// React Flow is heavy, so the pipeline view is its own chunk. Calling loadPipelineView()
// again (e.g. on hover) just preloads it; the module promise is cached.
const loadPipelineView = () => import("../pipeline/PipelineView");
const PipelineView = lazy(loadPipelineView);

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
  const fileCount = useCompressionStore((s) => s.summary.total);
  const queuedCount = useCompressionStore((s) => s.summary.queued);
  const hasQueued = queuedCount > 0;
  const clearFiles = useCompressionStore((s) => s.clearFiles);
  const isCompressing = useCompressionStore((s) => s.isCompressing);
  const isPaused = useCompressionStore((s) => s.isPaused);
  const showThumbnails = useCompressionStore((s) => s.showThumbnails);
  const toggleThumbnails = useCompressionStore((s) => s.toggleThumbnails);
  const setThumbnailPath = useCompressionStore((s) => s.setThumbnailPath);
  const fileView = useCompressionStore((s) => s.fileView);
  const setFileView = useCompressionStore((s) => s.setFileView);
  const { startCompression, pauseCompression, resumeCompression, cancelAllCompression } = useCompression();

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
    enabled: useVirtual && fileView === "list",
  });

  // Viewport-aware thumbnail generation.
  // Reads files/showThumbnails from the store snapshot to avoid capturing them
  // as deps — otherwise every setThumbnailPath call would recreate this callback
  // and refire the effects below.
  const generateVisibleThumbnails = useCallback(() => {
    const currentFiles = useCompressionStore.getState().files;
    const currentShow = useCompressionStore.getState().showThumbnails;
    // The scroll element is unmounted in pipeline view, so there is nothing visible to load
    if (useCompressionStore.getState().fileView === "pipeline") return;
    if (!currentShow || currentFiles.length === 0) return;

    const visibleItems = virtualizer.getVirtualItems();
    const needThumbnails = new Map<string, string>(); // path -> id

    for (const item of visibleItems) {
      const file = currentFiles[item.index];
      if (
        file &&
        !file.thumbnailPath &&
        !inFlightRef.current.has(file.id) &&
        !failedRef.current.has(file.id) &&
        file.mediaType !== "pdf"
      ) {
        needThumbnails.set(file.path, file.id);
        inFlightRef.current.add(file.id);
      }
    }

    if (needThumbnails.size === 0) return;

    // Results stream in one by one, so fast thumbnails show up without waiting for
    // the slowest item (a video seek or HEIC decode) in the same request.
    generateThumbnailsBatch([...needThumbnails.keys()], ({ path, thumbnailPath }) => {
      const id = needThumbnails.get(path);
      if (!id) return;
      inFlightRef.current.delete(id);
      if (thumbnailPath) {
        setThumbnailPath(id, thumbnailPath);
      } else {
        failedRef.current.add(id);
      }
    }).catch(() => {
      for (const id of needThumbnails.values()) {
        inFlightRef.current.delete(id);
      }
    });
  }, [virtualizer, setThumbnailPath]);

  // Generate thumbnails when the visible *range* changes (scrolling by whole rows
  // keeps the item count constant, so the count alone misses most scrolls),
  // when thumbnails are toggled on, or when files are added.
  const range = virtualizer.range;
  const rangeStart = range?.startIndex ?? 0;
  const rangeEnd = range?.endIndex ?? 0;
  useEffect(() => {
    if (fileView === "pipeline" || !showThumbnails || fileCount === 0) return;
    const timer = setTimeout(generateVisibleThumbnails, 100);
    return () => clearTimeout(timer);
  }, [fileView, showThumbnails, fileCount, rangeStart, rangeEnd, generateVisibleThumbnails]);

  // Clear in-flight tracking when thumbnails toggled off
  useEffect(() => {
    if (!showThumbnails) {
      inFlightRef.current.clear();
      failedRef.current.clear();
    }
  }, [showThumbnails]);

  // Invalidate cached row heights when switching between list/thumbnail views
  useEffect(() => {
    virtualizer.measure();
  }, [showThumbnails, virtualizer]);

  const handleAddMore = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ multiple: true, filters: dialogFilters() });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        const resolved = await scanForQueue(paths);
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
        const resolved = await scanForQueue([selected as string]);
        addResolvedPaths(resolved);
      }
    } catch {
      // cancelled
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      {/* Toolbar */}
      {/* Must stay on one line at the 800px minimum window (~448px panel), so the view
          toggle is icon-only and the count truncates rather than wraps. */}
      <div
        className="mb-3 flex items-center justify-between gap-3 border-b pb-3"
        style={{ borderColor: "var(--border)" }}
      >
        <span className="font-data min-w-0 truncate" style={{ color: "var(--text-muted)" }}>
          {fileCount} file{fileCount !== 1 ? "s" : ""}
        </span>
        <div className="flex shrink-0 gap-1.5">
          <div role="group" aria-label="View" className="flex">
            <ViewToggleButton active={fileView === "list"} view="list" onSelect={setFileView} label="List view">
              <List size={13} />
            </ViewToggleButton>
            <ViewToggleButton
              active={fileView === "pipeline"}
              view="pipeline"
              onSelect={setFileView}
              label="Pipeline view"
              onMouseEnter={() => void loadPipelineView()}
              onFocus={() => void loadPipelineView()}
            >
              <Workflow size={13} />
            </ViewToggleButton>
          </div>
          {fileView === "list" && (
            <ToolbarButton
              onClick={toggleThumbnails}
              title={showThumbnails ? "Hide thumbnails" : "Show thumbnails"}
              aria-label={showThumbnails ? "Hide thumbnails" : "Show thumbnails"}
            >
              {showThumbnails ? <LayoutList size={13} /> : <LayoutGrid size={13} />}
            </ToolbarButton>
          )}
          <ToolbarButton onClick={handleAddMore}>
            <Plus size={13} />
            Add
          </ToolbarButton>
          <ToolbarButton onClick={handleAddFolder}>
            <FolderOpen size={13} />
            Folder
          </ToolbarButton>
          <ToolbarButton
            onClick={clearFiles}
            danger
            disabled={isCompressing}
            title={isCompressing ? "Cancel compression first" : undefined}
          >
            <Trash2 size={13} />
            Clear
          </ToolbarButton>
        </div>
      </div>

      {/* File list / pipeline */}
      {fileView === "pipeline" ? (
        <Suspense fallback={<PipelineSkeleton />}>
          <PipelineView />
        </Suspense>
      ) : (
        <div ref={scrollRef} className="flex-1 overflow-y-auto pb-2">
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
      )}

      {/* Queue controls */}
      {!isCompressing && hasQueued && (
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
      {isCompressing && (
        <div className="mt-4 flex justify-center gap-2">
          {isPaused ? (
            <button
              className="btn-primary flex items-center gap-2 px-6 py-2.5 text-[15px]"
              onClick={resumeCompression}
            >
              <Play size={16} fill="currentColor" />
              Resume
            </button>
          ) : (
            <button
              className="btn-secondary flex items-center gap-2 px-6 py-2.5 text-[15px]"
              onClick={pauseCompression}
              title="Files already in progress finish; nothing new starts until you resume"
            >
              <Pause size={16} fill="currentColor" />
              Pause
            </button>
          )}
          <button
            className="btn-secondary flex items-center gap-2 px-6 py-2.5 text-[15px]"
            style={{ color: "var(--error)" }}
            onClick={cancelAllCompression}
          >
            <Square size={14} fill="currentColor" />
            Cancel All
          </button>
        </div>
      )}
    </div>
  );
}

function ToolbarButton({
  children,
  danger,
  disabled,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { danger?: boolean }) {
  return (
    <button
      className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium transition-colors duration-100"
      disabled={disabled}
      style={{
        color: danger ? "var(--error)" : "var(--text-secondary)",
        border: "1px solid var(--border)",
        backgroundColor: "transparent",
        opacity: disabled ? 0.4 : 1,
        cursor: disabled ? "not-allowed" : "pointer",
      }}
      onMouseEnter={(e) => {
        if (disabled) return;
        e.currentTarget.style.borderColor = danger ? "var(--error)" : "var(--border-hover)";
        e.currentTarget.style.backgroundColor = "var(--bg-tertiary)";
      }}
      onMouseLeave={(e) => {
        if (disabled) return;
        e.currentTarget.style.borderColor = "var(--border)";
        e.currentTarget.style.backgroundColor = "transparent";
      }}
      {...props}
    >
      {children}
    </button>
  );
}

// Icon-only segment; the label is its tooltip and accessible name.
function ViewToggleButton({
  active,
  view,
  label,
  onSelect,
  children,
  ...props
}: Omit<React.ButtonHTMLAttributes<HTMLButtonElement>, "onSelect"> & {
  active: boolean;
  view: FileView;
  label: string;
  onSelect: (view: FileView) => void;
}) {
  return (
    <button
      className="flex items-center px-2.5 py-1 text-xs font-medium transition-colors duration-100"
      aria-pressed={active}
      aria-label={label}
      title={label}
      style={{
        color: active ? "var(--accent)" : "var(--text-secondary)",
        border: "1px solid var(--border)",
        backgroundColor: active ? "var(--accent-glow)" : "transparent",
        cursor: "pointer",
        // overlap borders so the pair shares a single 1px divider
        marginLeft: view === "pipeline" ? -1 : 0,
      }}
      onClick={() => onSelect(view)}
      {...props}
    >
      {children}
    </button>
  );
}

// Reserves the pipeline area while its chunk loads so nothing jumps
function PipelineSkeleton() {
  return <div className="min-h-0 flex-1" style={{ border: "1px solid var(--border)" }} />;
}
