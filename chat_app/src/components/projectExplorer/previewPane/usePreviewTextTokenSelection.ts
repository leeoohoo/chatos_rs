import type React from 'react';

import { extractTokenAtColumn, getColumnFromPointer } from './previewPaneUtils';
import type { PreviewTokenSelection } from './previewPaneTypes';

interface UsePreviewTextTokenSelectionOptions {
  lineRefMap: React.MutableRefObject<Record<number, HTMLDivElement | null>>;
  rawLines: string[];
  onTokenSelection: (selection: PreviewTokenSelection | null) => void;
}

export const usePreviewTextTokenSelection = ({
  lineRefMap,
  rawLines,
  onTokenSelection,
}: UsePreviewTextTokenSelectionOptions) => {
  const handleLineMouseUp = (lineNumber: number, event: React.MouseEvent<HTMLDivElement>) => {
    if (typeof window === 'undefined' || typeof document === 'undefined') {
      return;
    }

    window.requestAnimationFrame(() => {
      const lineNode = lineRefMap.current[lineNumber];
      const selection = window.getSelection();
      if (!lineNode) {
        return;
      }

      if (selection && selection.rangeCount > 0) {
        const rawSelection = selection.toString();
        const token = rawSelection.trim();
        if (token && !rawSelection.includes('\n')) {
          const range = selection.getRangeAt(0);
          if (lineNode.contains(range.startContainer) && lineNode.contains(range.endContainer)) {
            const prefixRange = document.createRange();
            prefixRange.selectNodeContents(lineNode);
            prefixRange.setEnd(range.startContainer, range.startOffset);

            const lineText = rawLines[lineNumber - 1] ?? '';
            const leadingWhitespace = rawSelection.match(/^\s*/)?.[0].length ?? 0;
            const column = Math.max(
              1,
              Math.min(prefixRange.toString().length + leadingWhitespace + 1, lineText.length + 1),
            );

            onTokenSelection({
              token,
              line: lineNumber,
              column,
            });
            return;
          }
        }
      }

      const lineText = rawLines[lineNumber - 1] ?? '';
      const clickedColumn = getColumnFromPointer(lineNode, event);
      if (!clickedColumn) {
        return;
      }

      const extracted = extractTokenAtColumn(lineText, clickedColumn);
      if (!extracted) {
        return;
      }

      onTokenSelection({
        token: extracted.token,
        line: lineNumber,
        column: extracted.column,
      });
    });
  };

  return {
    handleLineMouseUp,
  };
};
