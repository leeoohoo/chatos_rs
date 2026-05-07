import { useCallback, useEffect, useState } from 'react';

import type {
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
  openLocation,
}: UseProjectExplorerCodeNavOptions): UseProjectExplorerCodeNavResult => {
  const [selectedToken, setSelectedToken] = useState<string | null>(null);
  const [selectedTokenLine, setSelectedTokenLine] = useState<number | null>(null);
  const [selectedTokenColumn, setSelectedTokenColumn] = useState<number | null>(null);

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
  } = useCodeNavResources({
    client,
    projectRootPath,
    selectedFilePath,
    clearTokenSelection,
  });

  useEffect(() => {
    clearTokenSelection();
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
    documentSymbols,
    documentSymbolsLoading,
    documentSymbolsError,
    handleTokenSelection,
    clearTokenSelection,
    requestDefinition,
    requestReferences,
    handleOpenNavLocation,
  };
};
