// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useState } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type {
  CodeNavLocation,
  CodeNavLocationsResult,
} from '../../../types';
import {
  buildCodeNavLocationId,
  normalizeCodeNavLocationsResult,
} from '../../../lib/domain/codeNav';
import type {
  CodeNavHistoryEntry,
  NavRequestKind,
  ProjectExplorerCodeNavApiClient,
} from './codeNavTypes';

interface UseCodeNavRequestsOptions {
  client: ProjectExplorerCodeNavApiClient;
  projectRootPath?: string | null;
  selectedFilePath?: string | null;
  selectedToken: string | null;
  selectedTokenLine: number | null;
  selectedTokenColumn: number | null;
  openLocation: (
    location: CodeNavLocation,
    options?: {
      preserveHistory?: boolean;
      targetLine?: number | null;
    },
  ) => Promise<void>;
  buildHistoryEntry: () => CodeNavHistoryEntry | null;
  pushHistoryEntry: (entry: CodeNavHistoryEntry) => void;
}

export const useCodeNavRequests = ({
  client,
  projectRootPath,
  selectedFilePath,
  selectedToken,
  selectedTokenLine,
  selectedTokenColumn,
  openLocation,
  buildHistoryEntry,
  pushHistoryEntry,
}: UseCodeNavRequestsOptions) => {
  const { t } = useI18n();
  const [navResult, setNavResult] = useState<CodeNavLocationsResult | null>(null);
  const [navRequestKind, setNavRequestKind] = useState<NavRequestKind | null>(null);
  const [navLoading, setNavLoading] = useState(false);
  const [navError, setNavError] = useState<string | null>(null);
  const [activeNavLocationId, setActiveNavLocationId] = useState<string | null>(null);

  const clearNavState = useCallback(() => {
    setNavResult(null);
    setNavRequestKind(null);
    setNavError(null);
    setActiveNavLocationId(null);
  }, []);

  const runNavRequest = useCallback(async (mode: NavRequestKind) => {
    if (!projectRootPath || !selectedFilePath || !selectedToken || !selectedTokenLine || !selectedTokenColumn) {
      setNavError(t('projectExplorer.codeNav.selectTokenFirst'));
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
        setNavError(mode === 'definition' ? t('projectExplorer.codeNav.noDefinition') : t('projectExplorer.codeNav.noReferences'));
      }
    } catch (error) {
      setNavResult(null);
      setNavError(error instanceof Error ? error.message : t('projectExplorer.codeNav.failed'));
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
    t,
  ]);

  const requestDefinition = useCallback(async () => {
    await runNavRequest('definition');
  }, [runNavRequest]);

  const requestReferences = useCallback(async () => {
    await runNavRequest('references');
  }, [runNavRequest]);

  const handleOpenNavLocation = useCallback(async (location: CodeNavLocation) => {
    const historyEntry = buildHistoryEntry();
    if (historyEntry) {
      pushHistoryEntry(historyEntry);
    }
    await openLocation(location, { preserveHistory: true });
    setActiveNavLocationId(buildCodeNavLocationId(location));
  }, [buildHistoryEntry, openLocation, pushHistoryEntry]);

  return {
    navResult,
    navRequestKind,
    navLoading,
    navError,
    activeNavLocationId,
    clearNavState,
    requestDefinition,
    requestReferences,
    handleOpenNavLocation,
  };
};
