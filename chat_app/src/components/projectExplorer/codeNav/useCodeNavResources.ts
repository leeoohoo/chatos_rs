import { useEffect, useState } from 'react';

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
  const [navCapabilities, setNavCapabilities] = useState<CodeNavCapabilities | null>(null);
  const [navCapabilitiesLoading, setNavCapabilitiesLoading] = useState(false);
  const [navCapabilitiesError, setNavCapabilitiesError] = useState<string | null>(null);
  const [documentSymbols, setDocumentSymbols] = useState<CodeNavDocumentSymbolsResult | null>(null);
  const [documentSymbolsLoading, setDocumentSymbolsLoading] = useState(false);
  const [documentSymbolsError, setDocumentSymbolsError] = useState<string | null>(null);

  useEffect(() => {
    clearTokenSelection();
    if (!projectRootPath || !selectedFilePath) {
      setNavCapabilities(null);
      setNavCapabilitiesError(null);
      setNavCapabilitiesLoading(false);
      setDocumentSymbols(null);
      setDocumentSymbolsError(null);
      setDocumentSymbolsLoading(false);
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
        setNavCapabilitiesError(error instanceof Error ? error.message : '获取代码导航能力失败');
      } finally {
        if (!cancelled) {
          setNavCapabilitiesLoading(false);
        }
      }
    };

    const loadDocumentSymbols = async () => {
      setDocumentSymbolsLoading(true);
      setDocumentSymbolsError(null);
      try {
        const raw = await client.getCodeNavDocumentSymbols(projectRootPath, selectedFilePath);
        if (cancelled) return;
        setDocumentSymbols(normalizeCodeNavDocumentSymbolsResult(raw));
      } catch (error) {
        if (cancelled) return;
        setDocumentSymbols(null);
        setDocumentSymbolsError(error instanceof Error ? error.message : '获取文件符号失败');
      } finally {
        if (!cancelled) {
          setDocumentSymbolsLoading(false);
        }
      }
    };

    void loadCapabilities();
    void loadDocumentSymbols();

    return () => {
      cancelled = true;
    };
  }, [clearTokenSelection, client, projectRootPath, selectedFilePath]);

  return {
    navCapabilities,
    navCapabilitiesLoading,
    navCapabilitiesError,
    documentSymbols,
    documentSymbolsLoading,
    documentSymbolsError,
  };
};
