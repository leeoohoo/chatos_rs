// @ts-nocheck
"use client";

import * as React from "react";
import QRCodeLib from "qrcode";
import { cn } from "@spell/lib/utils";

interface QRCodeProps extends React.SVGProps<SVGSVGElement> {
  value: string;
  size?: number;
  fgColor?: string;
  bgColor?: string;
  errorCorrectionLevel?: "L" | "M" | "Q" | "H";
  className?: string;
}

function isInFinderPattern(row: number, col: number, size: number): boolean {
  return (
    (row < 7 && col < 7) ||
    (row < 7 && col >= size - 7) ||
    (row >= size - 7 && col < 7)
  );
}

export function QRCode({
  value,
  size = 268,
  fgColor = "var(--foreground)",
  bgColor = "var(--background)",
  errorCorrectionLevel = "M",
  className,
  ...props
}: QRCodeProps) {
  const qrData = React.useMemo(() => {
    try {
      return QRCodeLib.create(value, { errorCorrectionLevel });
    } catch {
      return null;
    }
  }, [value, errorCorrectionLevel]);

  if (!qrData) {
    return null;
  }

  const moduleCount = qrData.modules.size;
  const moduleSize = size / moduleCount;
  const totalSize = size;
  const circleRadius = moduleSize * (1 / 3);

  const finderPositions: [number, number][] = [
    [0, 0],
    [0, moduleCount - 7],
    [moduleCount - 7, 0],
  ];

  const finderSize = 7 * moduleSize;
  const innerPadding = moduleSize;
  const innerWhiteSize = 5 * moduleSize;
  const innerBlackSize = 3 * moduleSize;

  const circles: { cx: number; cy: number }[] = [];

  for (let row = 0; row < moduleCount; row++) {
    for (let col = 0; col < moduleCount; col++) {
      if (qrData.modules.get(row, col) && !isInFinderPattern(row, col, moduleCount)) {
        circles.push({
          cx: (col + 0.5) * moduleSize,
          cy: (row + 0.5) * moduleSize,
        });
      }
    }
  }

  return (
    <svg
      width={totalSize}
      height={totalSize}
      viewBox={`0 0 ${totalSize} ${totalSize}`}
      xmlns="http://www.w3.org/2000/svg"
      aria-label={`QR code for ${value}`}
      className={cn("block", className)}
      {...props}
    >
      <rect width={totalSize} height={totalSize} fill={bgColor} rx="12" ry="12" />
      {finderPositions.map(([r, c]) => {
        const x = c * moduleSize;
        const y = r * moduleSize;
        return (
          <g key={`${r}-${c}`}>
            <rect
              x={x}
              y={y}
              width={finderSize}
              height={finderSize}
              fill={fgColor}
              rx="12"
              ry="12"
            />
            <rect
              x={x + innerPadding}
              y={y + innerPadding}
              width={innerWhiteSize}
              height={innerWhiteSize}
              fill={bgColor}
              rx="8"
              ry="8"
            />
            <rect
              x={x + innerPadding * 2}
              y={y + innerPadding * 2}
              width={innerBlackSize}
              height={innerBlackSize}
              fill={fgColor}
              rx="3"
              ry="3"
            />
          </g>
        );
      })}
      {circles.map(({ cx, cy }, i) => (
        <circle
          key={i}
          cx={cx}
          cy={cy}
          r={circleRadius}
          fill={fgColor}
        />
      ))}
    </svg>
  );
}
