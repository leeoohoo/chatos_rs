import { useCallback, useEffect, useRef, useState } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type {
  CodeNavCapabilities,
  CodeNavDocumentSymbolsResult,
} from '../../../types';
import {
  normalizeCodeNavCapabilities,
  normalizeCodeNavDocumentSymbolsResult,
} from '../../../lib/domain/codeNav';
import type { ProjectExplorerCodeNavApiClient } from './codeNavTypes';

interface UseCodeNavResourcesOptions {
  client: ProjectExplorerCodeNavApiClient;
  projectRootPath?: string | null;
  selectedFilePath?: string | null;
  clearTokenSelection: () => void;
}

export const useCodeNavResources = ({
  client,
  projectRootPath,
  selectedFilePath,
  clearTokenSelection,
}: UseCodeNavResourcesOptions) => {
  const { t } = useI18n();
  const [navCapabilities, setNavCapabilities] = useState<CodeNavCapabilities | null>(null);
  const [navCapabilitiesLoading, setNavCapabilitiesLoading] = useState(false);
  const [navCapabilitiesError, setNavCapabilitiesError] = useState<string | null>(null);
  const [documentSymbols, setDocumentSymbols] = useState<CodeNavDocumentSymbolsResult | null>(null);
  const [documentSymbolsLoading, setDocumentSymbolsLoading] = useState(false);
  const [documentSymbolsError, setDocumentSymbolsError] = useState<string | null>(null);
  const documentSymbolsRequestVersionRef = useRef(0);
  const documentSymbolsLoadedKeyRef = useRef<string | null>(null);
  const currentFileKey = projectRootPath && selectedFilePath
    ? `${projectRootPath}::${selectedFilePath}`
    : null;

  const resetDocumentSymbols = useCallback(() => {
    documentSymbolsLoadedKeyRef.current = null;
    setDocumentSymbols(null);
    setDocumentSymbolsError(null);
    setDocumentSymbolsLoading(false);
  }, []);

  const requestDocumentSymbols = useCallback(async () => {
    if (!projectRootPath || !selectedFilePath || !currentFileKey) {
      resetDocumentSymbols();
      return;
    }
    if (documentSymbolsLoadedKeyRef.current === currentFileKey) {
      return;
    }

    const requestVersion = ++documentSymbolsRequestVersionRef.current;
    setDocumentSymbolsLoading(true);
    setDocumentSymbolsError(null);
    try {
      const raw = await client.getCodeNavDocumentSymbols(projectRootPath, selectedFilePath);
      if (documentSymbolsRequestVersionRef.current !== requestVersion) {
        return;
      }
      setDocumentSymbols(normalizeCodeNavDocumentSymbolsResult(raw));
      documentSymbolsLoadedKeyRef.current = currentFileKey;
    } catch (error) {
      if (documentSymbolsRequestVersionRef.current !== requestVersion) {
        return;
      }
      setDocumentSymbols(null);
      setDocumentSymbolsError(error instanceof Error ? error.message : t('projectExplorer.codeNav.symbolsFailed'));
      documentSymbolsLoadedKeyRef.current = null;
    } finally {
      if (documentSymbolsRequestVersionRef.current === requestVersion) {
        setDocumentSymbolsLoading(false);
      }
    }
  }, [client, currentFileKey, projectRootPath, resetDocumentSymbols, selectedFilePath, t]);

  useEffect(() => {
    clearTokenSelection();
    if (!projectRootPath || !selectedFilePath) {
      setNavCapabilities(null);
      setNavCapabilitiesError(null);
      setNavCapabilitiesLoading(false);
      resetDocumentSymbols();
      return;
    }

    let cancelled = false;
    const loadCapabilities = async () => {
      setNavCapabilitiesLoading(true);
      setNavCapabilitiesError(null);
      try {
        const raw = await client.getCodeNavCapabilities(projectRootPath, selectedFilePath);
        if (cancelled) return;
        setNavCapabilities(normalizeCodeNavCapabilities(raw));
      } catch (error) {
        if (cancelled) return;
        setNavCapabilities(null);
        setNavCapabilitiesError(error instanceof Error ? error.message : t('projectExplorer.codeNav.capabilitiesFailed'));
      } finally {
        if (!cancelled) {
          setNavCapabilitiesLoading(false);
        }
      }
    };

    void loadCapabilities();
    resetDocumentSymbols();

    return () => {
      cancelled = true;
    };
  }, [clearTokenSelection, client, projectRootPath, resetDocumentSymbols, selectedFilePath, t]);

  return {
    navCapabilities,
    navCapabilitiesLoading,
    navCapabilitiesError,
    documentSymbols,
    documentSymbolsLoading,
    documentSymbolsError,
    requestDocumentSymbols,
  };
};
