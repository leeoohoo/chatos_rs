import { useCallback, useEffect, useState } from 'react';

import type {
  UseProjectExplorerSearchOptions,
  UseProjectExplorerSearchResult,
} from './search/projectExplorerSearchTypes';
import { useProjectSearchData } from './search/useProjectSearchData';
import { useProjectSearchNavigation } from './search/useProjectSearchNavigation';

export const useProjectExplorerSearch = ({
  client,
  projectRootPath,
}: UseProjectExplorerSearchOptions): UseProjectExplorerSearchResult => {
  const [searchQuery, setSearchQuery] = useState('');
  const [searchCaseSensitive, setSearchCaseSensitive] = useState(false);
  const [searchWholeWord, setSearchWholeWord] = useState(false);

  const {
    searchResults,
    searchLoading,
    searchError,
    setSearchError,
    searchTruncated,
    resetSearchData,
  } = useProjectSearchData({
    client,
    projectRootPath,
    searchQuery,
    searchCaseSensitive,
    searchWholeWord,
  });

  const {
    activeSearchHitId,
    activeSearchHitIndex,
    totalSearchHits,
    previewTargetLine,
    previewTargetLineRevision,
    setPreviewTargetLine,
    canOpenPreviousSearchHit,
    canOpenNextSearchHit,
    clearSearchNavigation,
    activateSearchHit,
    handleOpenSearchHit,
    openPreviousSearchHit,
    openNextSearchHit,
  } = useProjectSearchNavigation({
    searchResults,
  });

  const clearSearch = useCallback(() => {
    setSearchQuery('');
    resetSearchData();
    clearSearchNavigation();
  }, [clearSearchNavigation, resetSearchData]);

  const runSearchQuery = useCallback((query: string) => {
    const keyword = query.trim();
    if (!keyword) {
      clearSearch();
      return;
    }
    clearSearchNavigation();
    setSearchError(null);
    setSearchQuery(keyword);
  }, [clearSearch, clearSearchNavigation, setSearchError]);

  useEffect(() => {
    clearSearch();
  }, [clearSearch, projectRootPath]);

  useEffect(() => {
    clearSearchNavigation();
  }, [clearSearchNavigation, projectRootPath, searchCaseSensitive, searchQuery, searchWholeWord]);

  return {
    searchQuery,
    setSearchQuery,
    searchCaseSensitive,
    setSearchCaseSensitive,
    searchWholeWord,
    setSearchWholeWord,
    searchResults,
    searchLoading,
    searchError,
    searchTruncated,
    activeSearchHitId,
    activeSearchHitIndex,
    totalSearchHits,
    previewTargetLine,
    previewTargetLineRevision,
    setPreviewTargetLine,
    isSearchActive: searchQuery.trim().length > 0,
    canOpenPreviousSearchHit,
    canOpenNextSearchHit,
    runSearchQuery,
    clearSearch,
    clearSearchNavigation,
    activateSearchHit,
    handleOpenSearchHit,
    openPreviousSearchHit,
    openNextSearchHit,
  };
};
