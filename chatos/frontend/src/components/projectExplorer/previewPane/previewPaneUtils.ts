// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { MouseEvent } from 'react';

const isTokenChar = (value: string): boolean => /[A-Za-z0-9_$]/.test(value);

export const extractTokenAtColumn = (
  lineText: string,
  column: number,
): { token: string; column: number } | null => {
  if (!lineText) return null;
  const chars = Array.from(lineText);
  if (chars.length === 0) return null;

  let index = Math.max(0, Math.min(column - 1, chars.length - 1));
  if (!isTokenChar(chars[index]) && index > 0 && isTokenChar(chars[index - 1])) {
    index -= 1;
  }
  if (!isTokenChar(chars[index])) {
    return null;
  }

  let start = index;
  while (start > 0 && isTokenChar(chars[start - 1])) {
    start -= 1;
  }
  let end = index;
  while (end + 1 < chars.length && isTokenChar(chars[end + 1])) {
    end += 1;
  }

  return {
    token: chars.slice(start, end + 1).join(''),
    column: start + 1,
  };
};

export const getColumnFromPointer = (
  lineNode: HTMLDivElement,
  event: MouseEvent<HTMLDivElement>,
): number | null => {
  if (typeof document === 'undefined') {
    return null;
  }

  const doc = document as Document & {
    caretPositionFromPoint?: (
      x: number,
      y: number,
    ) => { offsetNode: Node; offset: number } | null;
    caretRangeFromPoint?: (x: number, y: number) => Range | null;
  };

  const caretPosition = doc.caretPositionFromPoint?.(event.clientX, event.clientY);
  if (caretPosition?.offsetNode && lineNode.contains(caretPosition.offsetNode)) {
    const prefixRange = document.createRange();
    prefixRange.selectNodeContents(lineNode);
    prefixRange.setEnd(caretPosition.offsetNode, caretPosition.offset);
    return prefixRange.toString().length + 1;
  }

  const caretRange = doc.caretRangeFromPoint?.(event.clientX, event.clientY);
  if (caretRange?.startContainer && lineNode.contains(caretRange.startContainer)) {
    const prefixRange = document.createRange();
    prefixRange.selectNodeContents(lineNode);
    prefixRange.setEnd(caretRange.startContainer, caretRange.startOffset);
    return prefixRange.toString().length + 1;
  }

  return null;
};
