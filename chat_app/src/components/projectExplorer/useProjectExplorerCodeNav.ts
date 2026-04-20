import { useCallback, useEffect, useState } from 'react';

import type {
  CodeNavCapabilities,
  CodeNavDocumentSymbolsResult,
  CodeNavLocation,
  CodeNavLocationsResult,
} from '../../types';
import {
  buildCodeNavLocationId,
  normalizeCodeNavCapabilities,
  normalizeCodeNavDocumentSymbolsResult,
  normalizeCodeNavLocationsResult,
} from './utils';

interface ProjectExplorerCodeNavApiClient {
  getCodeNavCapabilities: (projectRoot: string, filePath: string) => Promise<any>;
  getCodeNavDefinition: (data: {
    projectRoot: string;
    filePath: string;
    line: number;
    column: number;
  }) => Promise<any>;
  getCodeNavReferences: (data: {
    projectRoot: string;
    filePath: string;
    line: number;
    column: number;
  }) => Promise<any>;
  getCodeNavDocumentSymbols: (projectRoot: string, filePath: string) => Promise<any>;
}

interface UseProjectExplorerCodeNavOptions {
  client: ProjectExplorerCodeNavApiClient;
  projectRootPath?: string | null;
  selectedFilePath?: string | null;
  openLocation: (location: CodeNavLocation) => Promise<void>;
}

interface TokenSelection {
  token: string;
  line: number;
  column: number;
}

type NavRequestKind = 'definition' | 'references';

interface UseProjectExplorerCodeNavResult {
  navCapabilities: CodeNavCapabilities | null;
  navCapabilitiesLoading: boolean;
  navCapabilitiesError: string | null;
  selectedToken: string | null;
  selectedTokenLine: number | null;
  selectedTokenColumn: number | null;
  navResult: CodeNavLocationsResult | null;
  navRequestKind: NavRequestKind | null;
  navLoading: boolean;
  navError: string | null;
  activeNavLocationId: string | null;
  documentSymbols: CodeNavDocumentSymbolsResult | null;
  documentSymbolsLoading: boolean;
  documentSymbolsError: string | null;
  handleTokenSelection: (selection: TokenSelection | null) => void;
  clearTokenSelection: () => void;
  requestDefinition: () => Promise<void>;
  requestReferences: () => Promise<void>;
  handleOpenNavLocation: (location: CodeNavLocation) => Promise<void>;
}

export const useProjectExplorerCodeNav = ({
  client,
  projectRootPath,
  selectedFilePath,
  openLocation,
}: UseProjectExplorerCodeNavOptions): UseProjectExplorerCodeNavResult => {
  const [navCapabilities, setNavCapabilities] = useState<CodeNavCapabilities | null>(null);
  const [navCapabilitiesLoading, setNavCapabilitiesLoading] = useState(false);
  const [navCapabilitiesError, setNavCapabilitiesError] = useState<string | null>(null);
  const [selectedToken, setSelectedToken] = useState<string | null>(null);
  const [selectedTokenLine, setSelectedTokenLine] = useState<number | null>(null);
  const [selectedTokenColumn, setSelectedTokenColumn] = useState<number | null>(null);
  const [navResult, setNavResult] = useState<CodeNavLocationsResult | null>(null);
  const [navRequestKind, setNavRequestKind] = useState<NavRequestKind | null>(null);
  const [navLoading, setNavLoading] = useState(false);
  const [navError, setNavError] = useState<string | null>(null);
  const [activeNavLocationId, setActiveNavLocationId] = useState<string | null>(null);
  const [documentSymbols, setDocumentSymbols] = useState<CodeNavDocumentSymbolsResult | null>(null);
  const [documentSymbolsLoading, setDocumentSymbolsLoading] = useState(false);
  const [documentSymbolsError, setDocumentSymbolsError] = useState<string | null>(null);

  const clearSelectionOnly = useCallback(() => {
    setSelectedToken(null);
    setSelectedTokenLine(null);
    setSelectedTokenColumn(null);
  }, []);

  const clearTokenSelection = useCallback(() => {
    clearSelectionOnly();
    setNavResult(null);
    setNavRequestKind(null);
    setNavError(null);
    setActiveNavLocationId(null);
  }, [clearSelectionOnly]);

  useEffect(() => {
    clearTokenSelection();
  }, [clearTokenSelection, projectRootPath]);

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

  const handleTokenSelection = useCallback((selection: TokenSelection | null) => {
    if (!selection || !selection.token.trim()) {
      clearTokenSelection();
      return;
    }
    setSelectedToken(selection.token.trim());
    setSelectedTokenLine(selection.line);
    setSelectedTokenColumn(selection.column);
    setNavResult(null);
    setNavRequestKind(null);
    setNavError(null);
    setActiveNavLocationId(null);
  }, [clearTokenSelection]);

  const runNavRequest = useCallback(async (mode: NavRequestKind) => {
    if (!projectRootPath || !selectedFilePath || !selectedToken || !selectedTokenLine || !selectedTokenColumn) {
      setNavError('请先在代码中选中一个 token');
      return;
    }

    setNavLoading(true);
    setNavRequestKind(mode);
    setNavError(null);
    setActiveNavLocationId(null);
    try {
      const response = mode === 'definition'
        ? await client.getCodeNavDefinition({
          projectRoot: projectRootPath,
          filePath: selectedFilePath,
          line: selectedTokenLine,
          column: selectedTokenColumn,
        })
        : await client.getCodeNavReferences({
          projectRoot: projectRootPath,
          filePath: selectedFilePath,
          line: selectedTokenLine,
          column: selectedTokenColumn,
        });
      const normalized = normalizeCodeNavLocationsResult(response);
      setNavResult(normalized);
      if (normalized.locations.length === 0) {
        setNavError(mode === 'definition' ? '没有找到可跳转定义' : '没有找到引用结果');
      }
    } catch (error) {
      setNavResult(null);
      setNavError(error instanceof Error ? error.message : '代码导航失败');
    } finally {
      setNavLoading(false);
    }
  }, [
    client,
    projectRootPath,
    selectedFilePath,
    selectedToken,
    selectedTokenColumn,
    selectedTokenLine,
  ]);

  const requestDefinition = useCallback(async () => {
    await runNavRequest('definition');
  }, [runNavRequest]);

  const requestReferences = useCallback(async () => {
    await runNavRequest('references');
  }, [runNavRequest]);

  const handleOpenNavLocation = useCallback(async (location: CodeNavLocation) => {
    await openLocation(location);
    setActiveNavLocationId(buildCodeNavLocationId(location));
  }, [openLocation]);

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
