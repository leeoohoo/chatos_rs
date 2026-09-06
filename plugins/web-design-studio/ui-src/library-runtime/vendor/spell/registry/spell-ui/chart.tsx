// @ts-nocheck
"use client";

import * as React from "react";
import { cn } from "@spell/lib/utils";

const VIEWBOX_W = 640;
const VIEWBOX_H = 220;
const PAD_X = 0;
const PAD_Y_TOP = 24;
const PAD_Y_BOTTOM = 12;
const LINE_WIDTH = 2;
const CORNER_RADIUS = 2.5;
const TRANSITION = "200ms cubic-bezier(0.16, 1, 0.3, 1)";
const TOOLTIP_SHADOW = [
  "0 0 0 1px rgba(0, 0, 0, 0.04)",
  "0 1px 2px rgba(0, 0, 0, 0.04)",
  "0 4px 16px -4px rgba(0, 0, 0, 0.12)",
  "0 12px 32px -8px rgba(0, 0, 0, 0.08)",
].join(", ");
const DOT_SHADOW = "0 0 0 2px #FFF, 0 0 8px 2px rgba(0, 0, 0, 0.12)";

function buildRoundedPath(
  pts: Array<{ x: number; y: number }>,
  radius: number,
): string {
  if (pts.length === 0) return "";
  const f = (n: number) => n.toFixed(3);
  if (pts.length === 1) return `M${f(pts[0].x)} ${f(pts[0].y)}`;

  let d = `M${f(pts[0].x)} ${f(pts[0].y)}`;
  for (let i = 1; i < pts.length - 1; i++) {
    const prev = pts[i - 1];
    const curr = pts[i];
    const next = pts[i + 1];
    const inDx = curr.x - prev.x;
    const inDy = curr.y - prev.y;
    const inLen = Math.hypot(inDx, inDy) || 1;
    const outDx = next.x - curr.x;
    const outDy = next.y - curr.y;
    const outLen = Math.hypot(outDx, outDy) || 1;
    const r = Math.min(radius, inLen / 2, outLen / 2);
    const bx = curr.x - (inDx / inLen) * r;
    const by = curr.y - (inDy / inLen) * r;
    const ax = curr.x + (outDx / outLen) * r;
    const ay = curr.y + (outDy / outLen) * r;
    d += ` L${f(bx)} ${f(by)} Q${f(curr.x)} ${f(curr.y)} ${f(ax)} ${f(ay)}`;
  }
  const last = pts[pts.length - 1];
  d += ` L${f(last.x)} ${f(last.y)}`;
  return d;
}

export interface ChartProps {
  /** Numeric values to plot — one segment per pair of consecutive points. */
  data: number[];
  /** Optional label for each data point — shown at the top of the tooltip card. */
  labels?: string[];
  /** Series name shown next to the indicator dot in the tooltip. */
  name?: string;
  /** Line + dot color as a hex string (e.g. `"#0090FD"`). Default `"#0090FD"`. */
  color?: string;
  /** Maximum chart width in px. The chart preserves a 640:220 aspect ratio. Default `640`. */
  width?: number;
  /** Format the tooltip value. Receives the value and its index. */
  formatValue?: (value: number, index: number) => React.ReactNode;
  /** Index of the active point on initial render. Defaults to the last point. */
  defaultIndex?: number;
  /** Show X-axis tick labels at the bottom. Requires `labels`. Default `true`. */
  showXAxis?: boolean;
  /** Reveal the line + fill in color from left to the cursor (gray before, color after). Default `false`. */
  reveal?: boolean;
  /** Show the gray gradient fill under the line. Default `true`. */
  showFill?: boolean;
  /** Show the colored dot at the active data point. Default `true`. */
  showDot?: boolean;
  /** Smoothly animate the cursor, dot, and tooltip when the active point changes. Default `true`. */
  animated?: boolean;
  /** Target number of X-axis ticks. Default `6`. */
  tickCount?: number;
  /** Additional class names on the root container. */
  className?: string;
}

export function Chart({
  data,
  labels,
  name,
  color = "#0090FD",
  width = 640,
  formatValue = (v) => v.toLocaleString(),
  defaultIndex,
  showXAxis = true,
  tickCount = 6,
  reveal = false,
  showFill = true,
  showDot = true,
  animated = true,
  className,
}: ChartProps) {
  const transition = animated ? TRANSITION : "0ms";
  const reactId = React.useId();
  const grayFillId = `${reactId}-gf`;
  const clipId = `${reactId}-clip`;

  const root = React.useRef<HTMLDivElement>(null);
  const [activeIndex, setActiveIndex] = React.useState<number>(
    defaultIndex ?? Math.max(0, data.length - 1),
  );
  const [containerWidth, setContainerWidth] = React.useState<number>(width);

  React.useEffect(() => {
    if (!root.current) return;
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width;
      if (w) setContainerWidth(w);
    });
    ro.observe(root.current);
    return () => ro.disconnect();
  }, []);

  const points = React.useMemo(() => {
    const n = data.length;
    if (n === 0) return [];
    const minV = Math.min(...data);
    const maxV = Math.max(...data);
    const range = maxV - minV || 1;
    const innerW = VIEWBOX_W - 2 * PAD_X;
    const innerH = VIEWBOX_H - PAD_Y_TOP - PAD_Y_BOTTOM;
    return data.map((value, i) => ({
      value,
      index: i,
      x: PAD_X + (n === 1 ? innerW / 2 : (i / (n - 1)) * innerW),
      y: PAD_Y_TOP + (1 - (value - minV) / range) * innerH,
    }));
  }, [data]);

  const { strokePath, fillPath } = React.useMemo(() => {
    if (points.length === 0) return { strokePath: "", fillPath: "" };
    const stroke = buildRoundedPath(points, CORNER_RADIUS);
    const last = points[points.length - 1];
    const first = points[0];
    const baseY = VIEWBOX_H - PAD_Y_BOTTOM;
    const fill = `${stroke} L${last.x.toFixed(3)} ${baseY} L${first.x.toFixed(3)} ${baseY} Z`;
    return { strokePath: stroke, fillPath: fill };
  }, [points]);

  const active = points[Math.min(activeIndex, points.length - 1)] ?? points[0];
  const activeXPct = active ? active.x / VIEWBOX_W : 0;
  const activeYPct = active ? active.y / VIEWBOX_H : 0;

  function onMove(e: React.MouseEvent<HTMLDivElement>) {
    if (!root.current || points.length === 0) return;
    const rect = root.current.getBoundingClientRect();
    const rel = (e.clientX - rect.left) / rect.width;
    const innerLeft = PAD_X / VIEWBOX_W;
    const innerRight = (VIEWBOX_W - PAD_X) / VIEWBOX_W;
    const t = (rel - innerLeft) / (innerRight - innerLeft);
    const idx = Math.round(t * (points.length - 1));
    setActiveIndex(Math.max(0, Math.min(points.length - 1, idx)));
  }

  const axisVisible = showXAxis && !!labels && labels.length > 0;
  const tickIndices = React.useMemo(() => {
    if (!axisVisible) return [];
    const n = points.length;
    // Estimate label width ~80px and require min gap so labels don't overlap.
    const maxFit = Math.max(2, Math.floor(containerWidth / 80));
    const count = Math.min(tickCount, maxFit, n);
    if (count <= 1) return [0];
    return Array.from({ length: count }, (_, i) =>
      Math.round((i * (n - 1)) / (count - 1)),
    );
  }, [axisVisible, points.length, tickCount, containerWidth]);

  if (points.length === 0 || !active) return null;

  return (
    <div
      style={
        {
          maxWidth: width,
          "--spell-color": color,
        } as React.CSSProperties
      }
      className={cn(
        "w-full select-none",
        "[--spell-line:#c7c7c7] [--spell-badge:#e8e8e8]",
        "dark:[--spell-line:#4f4f4f] dark:[--spell-badge:#2d2d2d]",
        className,
      )}
    >
      <div
        ref={root}
        onMouseMove={onMove}
        className="relative w-full aspect-[640/220] touch-none"
      >
      <svg
        width="100%"
        height="100%"
        viewBox={`0 0 ${VIEWBOX_W} ${VIEWBOX_H}`}
        fill="none"
        preserveAspectRatio="xMidYMid meet"
        style={{ overflow: "visible" }}
      >
        <defs>
          <linearGradient
            id={grayFillId}
            x1={VIEWBOX_W / 2}
            y1={PAD_Y_TOP}
            x2={VIEWBOX_W / 2}
            y2={VIEWBOX_H}
            gradientUnits="userSpaceOnUse"
          >
            <stop stopColor="var(--spell-badge)" />
            <stop offset="1" stopColor="var(--spell-badge)" stopOpacity="0" />
          </linearGradient>
          <clipPath id={clipId} clipPathUnits="userSpaceOnUse">
            <rect
              x={0}
              y={0}
              width={VIEWBOX_W}
              height={VIEWBOX_H}
              style={{
                transform: `scaleX(${activeXPct})`,
                transformOrigin: "left center",
                transition: `transform ${transition}`,
              }}
            />
          </clipPath>
        </defs>

        {showFill && <path d={fillPath} fill={`url(#${grayFillId})`} />}

        {reveal ? (
          <>
            <path
              d={strokePath}
              stroke="var(--spell-line)"
              strokeWidth="2.2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
            <g clipPath={`url(#${clipId})`}>
              <path
                d={strokePath}
                stroke="var(--spell-color)"
                strokeWidth="2.2"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </g>
          </>
        ) : (
          <path
            d={strokePath}
            stroke="var(--spell-color)"
            strokeWidth="2.2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        )}
      </svg>

      {(() => {
        const cursorPx = activeXPct * containerWidth;
        const containerHeight = (containerWidth * VIEWBOX_H) / VIEWBOX_W;
        const cursorYpx = activeYPct * containerHeight;
        const tooltipOnLeft = activeXPct > 0.5;
        return (
          <>
            <div
              className="absolute pointer-events-none rounded-full bg-[var(--spell-line)] left-0"
              style={{
                width: LINE_WIDTH,
                top: `${(PAD_Y_TOP / VIEWBOX_H) * 100}%`,
                height: `${((VIEWBOX_H - PAD_Y_TOP - PAD_Y_BOTTOM / 2) / VIEWBOX_H) * 100}%`,
                transform: `translate3d(${cursorPx - LINE_WIDTH / 2}px, 0, 0)`,
                transition: `transform ${transition}`,
                willChange: "transform",
              }}
            />

            {showDot && (
              <div
                className="absolute pointer-events-none w-3 h-3 rounded-full bg-[var(--spell-color)] z-10 left-0 top-0"
                style={{
                  transform: `translate3d(${cursorPx - 6}px, ${cursorYpx - 6}px, 0)`,
                  boxShadow: DOT_SHADOW,
                  transition: `transform ${transition}`,
                  willChange: "transform",
                }}
              />
            )}

            <div
              className="absolute pointer-events-none z-20 grid min-w-32 items-start gap-1.5 rounded-lg bg-background px-2.5 py-1.5 text-xs left-0 top-0"
              style={{
                transform: tooltipOnLeft
                  ? `translate3d(calc(${cursorPx}px - 100% - 12px), calc(${cursorYpx}px - 50%), 0)`
                  : `translate3d(${cursorPx + 12}px, calc(${cursorYpx}px - 50%), 0)`,
                transition: `transform ${transition}`,
                willChange: "transform",
                boxShadow: TOOLTIP_SHADOW,
              }}
            >
        {labels?.[active.index] && (
          <div className="font-medium text-foreground">
            {labels[active.index]}
          </div>
        )}
        <div className="flex w-full items-center gap-2">
          <div
            className="h-2.5 w-2.5 shrink-0 rounded-[2px]"
            style={{ background: color }}
          />
          <div className="flex flex-1 items-center justify-between gap-3 leading-none">
            {name && (
              <span className="text-muted-foreground whitespace-nowrap">
                {name}
              </span>
            )}
            <span className="font-mono font-medium text-foreground tabular-nums ml-auto">
              {formatValue(active.value, active.index)}
            </span>
          </div>
        </div>
      </div>
          </>
        );
      })()}
      </div>

      {axisVisible && (
        <div className="relative h-5 mt-2">
          {tickIndices.map((i) => {
            const pt = points[i];
            const xPct = pt.x / VIEWBOX_W;
            return (
              <div
                key={i}
                className="absolute top-0 -translate-x-1/2 text-[11px] leading-none text-muted-foreground tabular-nums whitespace-nowrap"
                style={{ left: `${xPct * 100}%` }}
              >
                {labels![i]}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
