import { useCallback } from 'react';

interface UseProjectExplorerPreviewNavigationParams {
  handleTokenSelection: (selection: { token: string; line: number; column: number } | null) => void;
  setPreviewTargetLine: (line: number | null) => void;
}

export const useProjectExplorerPreviewNavigation = ({
  handleTokenSelection,
  setPreviewTargetLine,
}: UseProjectExplorerPreviewNavigationParams) => {
  const handlePreviewTokenSelection = useCallback((selection: {
    token: string;
    line: number;
    column: number;
  } | null) => {
    handleTokenSelection(selection);
    if (selection?.line) {
      setPreviewTargetLine(selection.line);
    }
  }, [handleTokenSelection, setPreviewTargetLine]);

  const handleOpenDocumentSymbol = useCallback((line: number) => {
    setPreviewTargetLine(line);
  }, [setPreviewTargetLine]);

  return {
    handlePreviewTokenSelection,
    handleOpenDocumentSymbol,
  };
};
