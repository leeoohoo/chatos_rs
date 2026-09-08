// @ts-nocheck
"use client";

import { motion } from "motion/react";
import { cn } from "@spell/lib/utils";

type From = "left" | "right" | "top" | "bottom";

interface HighlightedTextProps {
  children: React.ReactNode;
  className?: string;
  from?: From;
  delay?: number;
  inView?: boolean;
  once?: boolean;
}

const fromVariants = {
  left: {
    hidden: { x: "-100%" },
    visible: { x: "0%" },
  },
  right: {
    hidden: { x: "100%" },
    visible: { x: "0%" },
  },
  top: {
    hidden: { y: "-100%" },
    visible: { y: "0%" },
  },
  bottom: {
    hidden: { y: "100%" },
    visible: { y: "0%" },
  },
};

export function HighlightedText({
  children,
  className,
  from = "bottom",
  delay = 0,
  inView = false,
  once = true,
}: HighlightedTextProps) {
  const variants = fromVariants[from];

  return (
    <motion.span
      className={cn(
        "relative inline-flex overflow-hidden align-baseline",
        className,
      )}
      initial="hidden"
      whileInView={inView ? "visible" : undefined}
      animate={inView ? undefined : "visible"}
      viewport={{ once }}
    >
      <motion.span
        className="absolute inset-0 -left-[0.15em] -right-[0.18em] bg-black dark:bg-white z-0"
        variants={variants}
        transition={{
          type: "spring",
          damping: 30,
          stiffness: 300,
          delay,
        }}
      />
      <span className="relative z-10 mix-blend-difference text-white pl-[0.15em] pr-[0.18em]">
        {children}
      </span>
    </motion.span>
  );
}

export default HighlightedText;
