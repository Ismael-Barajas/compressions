import { useEffect, useId, useRef, useState, type RefObject } from "react";
import {
  Background,
  BackgroundVariant,
  Panel,
  ReactFlow,
  ReactFlowProvider,
  useNodesState,
  useReactFlow,
  useStore,
  type CoordinateExtent,
  type Edge,
  type Node,
} from "@xyflow/react";
import { RotateCcw } from "lucide-react";
import "@xyflow/react/dist/base.css";
import "./pipeline.css";
import { CheckNode, LaneNode, OutputNode, QueueNode, type LaneData } from "./nodes";
import { FlowEdge, type FlowEdgeData } from "./FlowEdge";
import type { MediaType } from "../../types/compression";

// Fixed layout (flow units). Sizes are set on the nodes so React Flow never has to
// measure them, and match the CSS (.pipeline-node fills the node box).
//
// The graph is kept compact (436 x 288) so it renders near 1:1 even in the smallest
// panel: at the 800x600 minimum window the canvas is ~448 x 300 while compressing,
// and fitting there must keep the 11px node text at >= 10px on screen. That is why
// Size check and Output share the last column (check above, output below) instead of
// adding a fourth column.
const LANE_H = 66;
const LANE_GAP = 8;
const LANES_H = 4 * LANE_H + 3 * LANE_GAP; // 288
const LANE_X = 120;
const LANE_W = 160;
const NODE_H = 72; // queue, size check, output
const END_X = 320; // size check + output column
const END_W = 116;
const END_GAP = 48; // vertical room for the check -> output edge
const END_Y = (LANES_H - (2 * NODE_H + END_GAP)) / 2; // the pair is centered on the lanes

const LANES: { mediaType: MediaType; title: string; engine: string; emptyText: string }[] = [
  // The frontend does not know which encoders were detected, so the text has to be
  // true on every machine: one at a time, two only when both are NVENC encodes.
  { mediaType: "video", title: "Video", engine: "FFmpeg · 1, or 2 on NVENC", emptyText: "No videos" },
  { mediaType: "image", title: "Image", engine: "Native · parallel", emptyText: "No images" },
  { mediaType: "pdf", title: "PDF", engine: "Ghostscript · parallel", emptyText: "No PDFs" },
  { mediaType: "audio", title: "Audio", engine: "FFmpeg · parallel", emptyText: "No audio" },
];

const nodes: Node[] = [
  { id: "queue", type: "queue", position: { x: 0, y: (LANES_H - NODE_H) / 2 }, width: 80, height: NODE_H, data: {} },
  ...LANES.map(
    (lane, i): Node<LaneData> => ({
      id: lane.mediaType,
      type: "lane",
      position: { x: LANE_X, y: i * (LANE_H + LANE_GAP) },
      width: LANE_W,
      height: LANE_H,
      data: lane,
    }),
  ),
  { id: "check", type: "check", position: { x: END_X, y: END_Y }, width: END_W, height: NODE_H, data: {} },
  {
    id: "output",
    type: "result",
    position: { x: END_X, y: END_Y + NODE_H + END_GAP },
    width: END_W,
    height: NODE_H,
    data: {},
  },
];

const edges: Edge<FlowEdgeData>[] = [
  ...LANES.flatMap(({ mediaType }): Edge<FlowEdgeData>[] => [
    { id: `queue-${mediaType}`, type: "flow", source: "queue", target: mediaType, data: { lane: mediaType, side: "in" } },
    { id: `${mediaType}-check`, type: "flow", source: mediaType, target: "check", data: { lane: mediaType, side: "out" } },
  ]),
  { id: "check-output", type: "flow", source: "check", target: "output", data: { side: "final" } },
];

// "result", not "output": React Flow styles its built-in input/output/default/group types.
const nodeTypes = { queue: QueueNode, lane: LaneNode, check: CheckNode, result: OutputNode };
const edgeTypes = { flow: FlowEdge };

// Pixel padding (not a fraction) so small panels don't lose extra room to it. The fit
// stays at most 1.2x even though the user may zoom in further.
const FIT_VIEW_OPTIONS = { padding: "12px", duration: 0, maxZoom: 1.2 } as const;
const PRO_OPTIONS = { hideAttribution: true };

// The user can drag the boxes and pan and zoom the canvas, but only so far, so the
// graph can't get lost off screen. The graph itself spans 436 x 288.
const GRAPH_W = END_X + END_W;
const NODE_EXTENT: CoordinateExtent = [
  [-120, -100],
  [GRAPH_W + 120, LANES_H + 100],
];
const PAN_EXTENT: CoordinateExtent = [
  [-320, -260],
  [GRAPH_W + 320, LANES_H + 260],
];

/**
 * Refits the graph whenever the panel resizes (sidebar toggle, window resize,
 * drag-over banner), unless the user has moved things; then their view is kept.
 */
function AutoFit({ userMoved }: { userMoved: RefObject<boolean> }) {
  const { fitView } = useReactFlow();
  const domNode = useStore((s) => s.domNode);

  useEffect(() => {
    if (!domNode) return;
    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      if (userMoved.current) return;
      frame = requestAnimationFrame(() => void fitView(FIT_VIEW_OPTIONS));
    });
    observer.observe(domNode);
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [domNode, fitView, userMoved]);

  return null;
}

function PipelineCanvas() {
  const [flowNodes, setFlowNodes, onNodesChange] = useNodesState(nodes);
  const { fitView } = useReactFlow();
  // Ref for AutoFit (no re-render needed), state for showing the reset button.
  const userMoved = useRef(false);
  const [moved, setMoved] = useState(false);
  const markMoved = () => {
    userMoved.current = true;
    setMoved(true);
  };

  const reset = () => {
    userMoved.current = false;
    setMoved(false);
    setFlowNodes(nodes);
    const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    // Next frame, so the fit sees the nodes back in their places.
    requestAnimationFrame(() => void fitView({ ...FIT_VIEW_OPTIONS, duration: reduceMotion ? 0 : 250 }));
  };

  return (
    <ReactFlow
      className="pipeline"
      style={{ position: "absolute", inset: 0 }}
      nodes={flowNodes}
      onNodesChange={onNodesChange}
      edges={edges}
      nodeTypes={nodeTypes}
      edgeTypes={edgeTypes}
      fitView
      fitViewOptions={FIT_VIEW_OPTIONS}
      minZoom={0.5}
      maxZoom={1.6}
      nodeExtent={NODE_EXTENT}
      translateExtent={PAN_EXTENT}
      onNodeDragStart={markMoved}
      // event is null when the move came from code (fitView), not the user.
      onMoveStart={(event) => {
        if (event) markMoved();
      }}
      nodesDraggable
      nodesConnectable={false}
      nodesFocusable={false}
      edgesFocusable={false}
      elementsSelectable={false}
      panOnDrag
      panOnScroll={false}
      zoomOnScroll
      zoomOnPinch
      zoomOnDoubleClick={false}
      deleteKeyCode={null}
      selectionKeyCode={null}
      multiSelectionKeyCode={null}
      // Defaults to Space, which would swallow the app's global Space-to-start shortcut.
      panActivationKeyCode={null}
      zoomActivationKeyCode={null}
      // The attribution badge is a link that would navigate the Tauri webview away.
      proOptions={PRO_OPTIONS}
    >
      <Background variant={BackgroundVariant.Dots} gap={16} size={1} color="var(--border)" />
      <AutoFit userMoved={userMoved} />
      {moved && (
        <Panel position="top-right" className="!m-2">
          <button
            type="button"
            onClick={reset}
            title="Put the boxes and view back"
            className="flex items-center gap-1.5 border px-2 py-1 text-[11px] transition-colors duration-100"
            style={{
              borderColor: "var(--border)",
              backgroundColor: "var(--bg-secondary)",
              color: "var(--text-secondary)",
            }}
          >
            <RotateCcw size={12} />
            Reset layout
          </button>
        </Panel>
      )}
    </ReactFlow>
  );
}

export default function PipelineView() {
  const descriptionId = useId();

  return (
    <div className="relative min-h-0 flex-1" role="region" aria-label="Compression pipeline" aria-describedby={descriptionId}>
      <p id={descriptionId} className="sr-only">
        Queued files flow into one lane per media type. Videos encode with FFmpeg one at a time, or two at a time when
        both use an NVIDIA GPU encoder (NVENC); images encode in parallel natively; PDFs in parallel with Ghostscript; audio in parallel with FFmpeg. Every result then passes a
        size check that keeps the original when the output isn't smaller, before it is written to the output folder.
      </p>
      <ReactFlowProvider>
        <PipelineCanvas />
      </ReactFlowProvider>
    </div>
  );
}
