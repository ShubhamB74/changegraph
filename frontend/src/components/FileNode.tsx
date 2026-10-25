"use client";

import { Handle, Position } from "@xyflow/react";
import type { Node, NodeProps } from "@xyflow/react";
import { memo } from "react";
import { NODE_HEIGHT, NODE_WIDTH } from "../lib/layout.ts";
import type { FileNodeData } from "../lib/flow-graph.ts";
import { STATE_STYLE, TEST_COLOR } from "../lib/theme.ts";

export type { FileNodeData };
export type FileFlowNode = Node<FileNodeData, "file">;

function FileNodeView({ data }: NodeProps<FileFlowNode>) {
  const s = STATE_STYLE[data.state];
  return (
    <div
      className="flex flex-col justify-center overflow-hidden rounded-md bg-ink-panel px-2.5"
      style={{
        width: NODE_WIDTH,
        height: NODE_HEIGHT,
        borderWidth: data.selected ? 2.5 : 1.5,
        borderStyle: s.border,
        borderColor: data.selected ? "#F4F7FB" : s.color,
        opacity: data.dimmed ? 0.4 : 1,
        boxShadow: data.matched ? "0 0 0 3px rgba(127,178,255,0.55)" : undefined,
      }}
    >
      {/* Arrows point from importer to imported, so the importing file connects on its left. */}
      <Handle type="source" position={Position.Left} isConnectable={false} style={{ opacity: 0 }} />
      <Handle type="target" position={Position.Right} isConnectable={false} style={{ opacity: 0 }} />
      <div className="flex items-center gap-1.5">
        <span aria-hidden style={{ color: s.color }}>
          {s.glyph}
        </span>
        <span className="truncate font-mono text-[13px] font-medium" title={`${data.dir}/${data.base}`}>
          {data.base}
        </span>
        {data.category === "test" && (
          <span className="shrink-0 rounded px-1 text-[10px] leading-4" style={{ color: TEST_COLOR, border: `1px solid ${TEST_COLOR}` }}>
            test
          </span>
        )}
        {data.recommendedRank !== null && (
          <span
            className="shrink-0 text-[11px]"
            style={{ color: data.basis === "known" ? TEST_COLOR : "#FF8A5B" }}
            title={data.basis === "known" ? "Recommended test (known relationship)" : "Recommended test (heuristic guess)"}
          >
            ▶{data.recommendedRank}
          </span>
        )}
      </div>
      <div className="flex items-center gap-2 text-[11px] text-ink-dim">
        <span className="truncate font-mono">{data.dir || "."}</span>
        {data.distance !== null && data.distance > 0 && <span className="shrink-0">d{data.distance}</span>}
        {data.parse && data.parse !== "ok" && <span className="shrink-0 text-signal-limit">{data.parse}</span>}
      </div>
    </div>
  );
}

export const FileNode = memo(FileNodeView);
