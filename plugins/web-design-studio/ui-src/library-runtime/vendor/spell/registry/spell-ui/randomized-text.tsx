// @ts-nocheck
"use client";

import { motion } from "motion/react";
import { useMemo } from "react";

type SplitType = "words" | "chars";

interface RandomizedTextProps {
  children: string;
  className?: string;
  split?: SplitType;
  delay?: number;
  inView?: boolean;
  once?: boolean;
}

export function RandomizedText({
  children,
  className = "",
  split = "words",
  delay = 0.2,
  inView = false,
  once = true,
}: RandomizedTextProps) {

  const expoOut = (t: number): number => {
    return t === 1 ? 1 : 1 - Math.pow(2, -10 * t);
  };

  const elements = useMemo(() => {
    if (split === "chars") {
      return children.split("").map((char, i) => ({
        content: char === " " ? "\u00A0" : char,
        key: `char-${i}`,
      }));
    }
    return children.split(" ").map((word, i) => ({
      content: word,
      key: `word-${i}`,
    }));
  }, [children, split]);

  const randomizedDelays = useMemo(() => {
    return elements.map(() =>
      delay + Math.random() * 0.2 + Math.random() * 0.03
    );
  }, [elements.length, delay]);

  const variants = {
    hidden: { opacity: 0 },
    visible: { opacity: 1 },
  };

  return (
    <motion.span
      className={className}
      aria-label={children}
      style={{ display: "inline-block", wordBreak: "break-word" }}
      initial="hidden"
      whileInView={inView ? "visible" : undefined}
      animate={inView ? undefined : "visible"}
      viewport={{ once }}
    >
      {elements.map((element, i) => (
        <motion.span
          key={element.key}
          variants={variants}
          transition={{
            duration: 1.2,
            delay: randomizedDelays[i],
            ease: expoOut,
          }}
          style={{ display: split === "words" ? "inline-block" : "inline" }}
          className={split === "words" ? "mr-[0.25em]" : ""}
        >
          {element.content}
        </motion.span>
      ))}
    </motion.span>
  );
}
