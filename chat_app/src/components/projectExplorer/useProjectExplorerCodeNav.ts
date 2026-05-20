import { useCallback, useEffect, useState } from 'react';

import type {
  CodeNavHistoryEntry,
  TokenSelection,
  UseProjectExplorerCodeNavOptions,
  UseProjectExplorerCodeNavResult,
} from './codeNav/codeNavTypes';
import { useCodeNavRequests } from './codeNav/useCodeNavRequests';
import { useCodeNavResources } from './codeNav/useCodeNavResources';

export const useProjectExplorerCodeNav = ({
  client,
  projectRootPath,
  selectedFilePath,
  targetLine,
  openLocation,
}: UseProjectExplorerCodeNavOptions): UseProjectExplorerCodeNavResult => {
  const [selectedToken, setSelectedToken] = useState<string | null>(null);
  const [selectedTokenLine, setSelectedTokenLine] = useState<number | null>(null);
  const [selectedTokenColumn, setSelectedTokenColumn] = useState<number | null>(null);
  const [navHistory, setNavHistory] = useState<CodeNavHistoryEntry[]>([]);

  const goBackFromNav = useCallback(async () => {
    const previous = navHistory[navHistory.length - 1];
    if (!previous) {
      return;
    }
    setNavHistory((entries) => entries.slice(0, -1));
    await openLocation(
      {
        path: previous.path,
        relativePath: previous.path,
        line: previous.targetLine || 1,
        column: 1,
        endLine: previous.targetLine || 1,
        endColumn: 1,
        preview: '',
        score: 0,
      },
      {
        preserveHistory: false,
        targetLine: previous.targetLine,
      },
    );
  }, [navHistory, openLocation]);

  const {
    navResult,
    navRequestKind,
    navLoading,
    navError,
    activeNavLocationId,
    clearNavState,
    requestDefinition,
    requestReferences,
    handleOpenNavLocation,
  } = useCodeNavRequests({
    client,
    projectRootPath,
    selectedFilePath,
    selectedToken,
    selectedTokenLine,
    selectedTokenColumn,
    openLocation,
    buildHistoryEntry: () => {
      if (!selectedFilePath) {
        return null;
      }
      return {
        path: selectedFilePath,
        targetLine,
      };
    },
    pushHistoryEntry: (entry) => {
      setNavHistory((entries) => {
        const last = entries[entries.length - 1];
        if (
          last
          && last.path === entry.path
          && last.targetLine === entry.targetLine
        ) {
          return entries;
        }
        return [...entries, entry];
      });
    },
  });

  const clearSelectionOnly = useCallback(() => {
    setSelectedToken(null);
    setSelectedTokenLine(null);
    setSelectedTokenColumn(null);
  }, []);

  const clearTokenSelection = useCallback(() => {
    clearSelectionOnly();
    clearNavState();
  }, [clearNavState, clearSelectionOnly]);

  const {
    navCapabilities,
    navCapabilitiesLoading,
    navCapabilitiesError,
    documentSymbols,
    documentSymbolsLoading,
    documentSymbolsError,
    requestDocumentSymbols,
  } = useCodeNavResources({
    client,
    projectRootPath,
    selectedFilePath,
    clearTokenSelection,
  });

  useEffect(() => {
    clearTokenSelection();
    setNavHistory([]);
  }, [clearTokenSelection, projectRootPath]);

  const handleTokenSelection = useCallback((selection: TokenSelection | null) => {
    if (!selection || !selection.token.trim()) {
      clearTokenSelection();
      return;
    }
    setSelectedToken(selection.token.trim());
    setSelectedTokenLine(selection.line);
    setSelectedTokenColumn(selection.column);
    clearNavState();
  }, [clearNavState, clearTokenSelection]);

  return {
    navCapabilities,
    navCapabilitiesLoading,
    navCapabilitiesError,
    selectedToken,
    selectedTokenLine,
    selectedTokenColumn,
    navResult,
    navRequestKind,
    navLoading,
    navError,
    activeNavLocationId,
    canGoBackFromNav: navHistory.length > 0,
    documentSymbols,
    documentSymbolsLoading,
    documentSymbolsError,
    requestDocumentSymbols,
    handleTokenSelection,
    clearTokenSelection,
    requestDefinition,
    requestReferences,
    handleOpenNavLocation,
    goBackFromNav,
  };
};
