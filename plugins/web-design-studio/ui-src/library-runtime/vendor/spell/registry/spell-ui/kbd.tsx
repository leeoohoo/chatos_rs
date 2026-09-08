// @ts-nocheck
"use client";

import { cn } from "@spell/lib/utils";
import { useEffect, useState } from "react";
import { useHotkeys } from "react-hotkeys-hook";

type KeyItem = string | { display: string; key: string };

interface KbdProps {
  keys: KeyItem[];
  className?: string;
  active?: boolean;
  listenToKeyboard?: boolean;
}

const keySymbolMap = {
  command: "⌘",
  cmd: "⌘",
  control: "⌃",
  ctrl: "⌃",
  alt: "⌥",
  option: "⌥",
  space: "␣",
  arrowleft: "←",
  left: "←",
  arrowdown: "↓",
  down: "↓",
  arrowup: "↑",
  up: "↑",
  arrowright: "→",
  right: "→",
} as const;

const keyHotkeyMap: Record<string, string> = {
  command: "meta",
  cmd: "meta",
  control: "ctrl",
  ctrl: "ctrl",
  alt: "alt",
  option: "alt",
  shift: "shift",
  enter: "enter",
  return: "enter",
  space: "space",
  arrowleft: "left",
  left: "left",
  arrowdown: "down",
  down: "down",
  arrowup: "up",
  up: "up",
  arrowright: "right",
  right: "right",
};

export function Kbd({ keys = [], className, active, listenToKeyboard = false }: KbdProps) {
  const [isPressed, setIsPressed] = useState(false);

  const getKeyDisplay = (item: KeyItem): string => {
    const key = typeof item === "string" ? item : item.display;
    const lowerKey = key.toLowerCase();
    return keySymbolMap[lowerKey as keyof typeof keySymbolMap] || key.toUpperCase();
  };

  const getHotkeyString = (): string => {
    return keys
      .map((item) => {
        const key = typeof item === "string" ? item : item.key;
        const lowerKey = key.toLowerCase();
        return keyHotkeyMap[lowerKey] || lowerKey;
      })
      .join("+");
  };

  useHotkeys(
    getHotkeyString(),
    () => setIsPressed(true),
    {
      enabled: listenToKeyboard,
      keydown: true,
      keyup: false,
      preventDefault: false,
    },
    [keys, listenToKeyboard]
  );

  useEffect(() => {
    if (!listenToKeyboard) return;

    const handleKeyUp = () => {
      setIsPressed(false);
    };

    const handleBlur = () => {
      setIsPressed(false);
    };

    window.addEventListener("keyup", handleKeyUp);
    window.addEventListener("blur", handleBlur);

    return () => {
      window.removeEventListener("keyup", handleKeyUp);
      window.removeEventListener("blur", handleBlur);
    };
  }, [listenToKeyboard]);

  const isActive = active || isPressed;

  return (
    <kbd
      className={cn(
        "box-border align-text-top whitespace-nowrap select-none cursor-default tracking-tight rounded-[0.35em] min-w-[1.75em] shrink-0 justify-center items-center pb-[0.05em] px-[0.5em] text-[0.75em] font-normal leading-[1.7em] inline-flex relative -top-[0.03em] transition-all duration-100",
        isActive
          ? "bg-background text-foreground translate-y-[0.05em] shadow-[inset_0_0.05em_rgba(255,255,255,0.95),inset_0_0.05em_0.2em_rgba(0,0,0,0.1),0_0_0_0.05em_rgba(0,0,0,0.134)] dark:shadow-[inset_0_0.05em_0.2em_rgba(0,0,0,0.3),0_0_0_0.05em_rgba(255,255,255,0.134)]"
          : "bg-background text-foreground shadow-[inset_0_-0.05em_0.5em_rgba(0,0,0,0.034),inset_0_0.05em_rgba(255,255,255,0.95),inset_0_0.25em_0.5em_rgba(0,0,0,0.034),inset_0_-0.05em_rgba(0,0,0,0.172),0_0_0_0.05em_rgba(0,0,0,0.134),0_0.08em_0.17em_rgba(0,0,0,0.231)] dark:shadow-[inset_0_-0.05em_0.5em_rgba(255,255,255,0.034),inset_0_0.05em_rgba(255,255,255,0.1),inset_0_0.25em_0.5em_rgba(255,255,255,0.034),inset_0_-0.05em_rgba(255,255,255,0.172),0_0_0_0.05em_rgba(255,255,255,0.134),0_0.08em_0.17em_rgba(255,255,255,0.231)]",
        className
      )}
    >
      {keys.map((item, index) => (
        <span key={index} className={index > 0 ? "ml-0.5" : ""}>
          {getKeyDisplay(item)}
        </span>
      ))}
    </kbd>
  );
}
