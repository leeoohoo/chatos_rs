// @ts-nocheck
"use client";

import * as React from "react";
import { cn } from "@spell/lib/utils";

interface BarsSpinnerProps extends React.HTMLAttributes<HTMLDivElement> {
  size?: number;
  color?: string;
}

export const BarsSpinner = React.forwardRef<HTMLDivElement, BarsSpinnerProps>(
  ({ className, size = 20, color = "currentColor", ...props }, ref) => {
    const bars = Array(12).fill(0);

    return (
      <div
        ref={ref}
        className={cn("wrapper", className)}
        style={{
          ["--spinner-size" as string]: `${size}px`,
          ["--spinner-color" as string]: color,
        }}
        {...props}
      >
        <div className="spinner">
          {bars.map((_, i) => (
            <div className="bar" key={`spinner-bar-${i}`} />
          ))}
        </div>
        <style jsx>{`
          .wrapper {
            height: var(--spinner-size, 20px);
            width: var(--spinner-size, 20px);
          }

          .spinner {
            position: relative;
            top: 50%;
            left: 50%;
            height: var(--spinner-size, 20px);
            width: var(--spinner-size, 20px);
          }

          .bar {
            animation: spin 1.2s linear infinite;
            background: var(--spinner-color);
            border-radius: 6px;
            height: 8%;
            left: -10%;
            position: absolute;
            top: -3.9%;
            width: 24%;
          }

          .bar:nth-child(1) {
            animation-delay: -1.2s;
            transform: rotate(0.0001deg) translate(146%);
          }

          .bar:nth-child(2) {
            animation-delay: -1.1s;
            transform: rotate(30deg) translate(146%);
          }

          .bar:nth-child(3) {
            animation-delay: -1s;
            transform: rotate(60deg) translate(146%);
          }

          .bar:nth-child(4) {
            animation-delay: -0.9s;
            transform: rotate(90deg) translate(146%);
          }

          .bar:nth-child(5) {
            animation-delay: -0.8s;
            transform: rotate(120deg) translate(146%);
          }

          .bar:nth-child(6) {
            animation-delay: -0.7s;
            transform: rotate(150deg) translate(146%);
          }

          .bar:nth-child(7) {
            animation-delay: -0.6s;
            transform: rotate(180deg) translate(146%);
          }

          .bar:nth-child(8) {
            animation-delay: -0.5s;
            transform: rotate(210deg) translate(146%);
          }

          .bar:nth-child(9) {
            animation-delay: -0.4s;
            transform: rotate(240deg) translate(146%);
          }

          .bar:nth-child(10) {
            animation-delay: -0.3s;
            transform: rotate(270deg) translate(146%);
          }

          .bar:nth-child(11) {
            animation-delay: -0.2s;
            transform: rotate(300deg) translate(146%);
          }

          .bar:nth-child(12) {
            animation-delay: -0.1s;
            transform: rotate(330deg) translate(146%);
          }

          @keyframes spin {
            0% {
              opacity: 1;
            }
            100% {
              opacity: 0.15;
            }
          }
        `}</style>
      </div>
    );
  },
);

BarsSpinner.displayName = "SpellUI.BarsSpinner";

export type { BarsSpinnerProps };
