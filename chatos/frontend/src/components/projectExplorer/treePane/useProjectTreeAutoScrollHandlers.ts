// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type React from 'react';

interface UseProjectTreeAutoScrollHandlersOptions {
  draggingEntryPath: string | null;
  treeScrollRef: React.MutableRefObject<HTMLDivElement | null>;
  onStartDragAutoScroll: (velocity: number) => void;
  onClearDragAutoScroll: () => void;
}

export const useProjectTreeAutoScrollHandlers = ({
  draggingEntryPath,
  treeScrollRef,
  onStartDragAutoScroll,
  onClearDragAutoScroll,
}: UseProjectTreeAutoScrollHandlersOptions) => {
  const handleContainerDragOver = (event: React.DragEvent<HTMLDivElement>) => {
    if (!draggingEntryPath) return;
    const container = treeScrollRef.current;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const threshold = Math.max(28, Math.min(64, rect.height / 3));
    let velocity = 0;

    if (event.clientY < rect.top + threshold) {
      const ratio = (rect.top + threshold - event.clientY) / threshold;
      velocity = -Math.max(4, Math.round(22 * ratio));
    } else if (event.clientY > rect.bottom - threshold) {
      const ratio = (event.clientY - (rect.bottom - threshold)) / threshold;
      velocity = Math.max(4, Math.round(22 * ratio));
    }

    if (velocity !== 0) {
      event.preventDefault();
      onStartDragAutoScroll(velocity);
    } else {
      onClearDragAutoScroll();
    }
  };

  const handleContainerDragLeave = (event: React.DragEvent<HTMLDivElement>) => {
    const nextTarget = event.relatedTarget as Node | null;
    if (nextTarget && (event.currentTarget as HTMLElement).contains(nextTarget)) {
      return;
    }
    onClearDragAutoScroll();
  };

  const handleContainerDrop = () => {
    onClearDragAutoScroll();
  };

  return {
    handleContainerDragLeave,
    handleContainerDragOver,
    handleContainerDrop,
  };
};
