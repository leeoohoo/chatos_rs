// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useState } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type { ProjectSearchHit } from '../../../types';
import { normalizeProjectSearchHit } from '../../../lib/domain/projectSearch';
import type { ProjectExplorerSearchApiClient } from './projectExplorerSearchTypes';

interface UseProjectSearchDataOptions {
  client: ProjectExplorerSearchApiClient;
  projectRootPath?: string | null;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
}

export const useProjectSearchData = ({
  client,
  projectRootPath,
  searchQuery,
  searchCaseSensitive,
  searchWholeWord,
}: UseProjectSearchDataOptions) => {
  const { t } = useI18n();
  const [searchResults, setSearchResults] = useState<ProjectSearchHit[]>([]);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [searchTruncated, setSearchTruncated] = useState(false);

  const resetSearchData = useCallback(() => {
    setSearchResults([]);
    setSearchError(null);
    setSearchTruncated(false);
    setSearchLoading(false);
  }, []);

  useEffect(() => {
    const keyword = searchQuery.trim();
    if (!projectRootPath || !keyword) {
      resetSearchData();
      return;
    }

    let cancelled = false;
    const timer = window.setTimeout(async () => {
      setSearchLoading(true);
      setSearchError(null);
      try {
        const data = await client.searchFsContent(projectRootPath, keyword, {
          limit: 200,
          caseSensitive: searchCaseSensitive,
          wholeWord: searchWholeWord,
        });
        if (cancelled) return;

        const entries = Array.isArray(data?.entries)
          ? data.entries.map(normalizeProjectSearchHit).filter((hit: ProjectSearchHit) => hit.path)
          : [];
        setSearchResults(entries);
        setSearchTruncated(Boolean(data?.truncated));
      } catch (error) {
        if (cancelled) return;
        setSearchResults([]);
        setSearchTruncated(false);
        setSearchError(error instanceof Error ? error.message : t('projectExplorer.search.failed'));
      } finally {
        if (!cancelled) {
          setSearchLoading(false);
        }
      }
    }, 180);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [
    client,
    projectRootPath,
    resetSearchData,
    searchCaseSensitive,
    searchQuery,
    searchWholeWord,
    t,
  ]);

  return {
    searchResults,
    setSearchResults,
    searchLoading,
    setSearchLoading,
    searchError,
    setSearchError,
    searchTruncated,
    setSearchTruncated,
    resetSearchData,
  };
};
