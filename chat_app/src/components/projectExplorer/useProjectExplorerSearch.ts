import { useCallback, useEffect, useState, type Dispatch, type SetStateAction } from 'react';

import type { FsEntry, ProjectSearchHit } from '../../types';
import {
  buildProjectSearchHitId,
  normalizeProjectSearchHit,
} from './utils';

interface ProjectExplorerSearchApiClient {
  searchFsContent: (
    path: string,
    query: string,
    options?: { limit?: number; caseSensitive?: boolean; wholeWord?: boolean }
  ) => Promise<any>;
}

interface UseProjectExplorerSearchOptions {
  client: ProjectExplorerSearchApiClient;
  projectRootPath?: string | null;
}

interface UseProjectExplorerSearchResult {
  searchQuery: string;
  setSearchQuery: Dispatch<SetStateAction<string>>;
  searchCaseSensitive: boolean;
  setSearchCaseSensitive: Dispatch<SetStateAction<boolean>>;
  searchWholeWord: boolean;
  setSearchWholeWord: Dispatch<SetStateAction<boolean>>;
  searchResults: ProjectSearchHit[];
  searchLoading: boolean;
  searchError: string | null;
  searchTruncated: boolean;
  activeSearchHitId: string | null;
  activeSearchHitIndex: number;
  totalSearchHits: number;
  previewTargetLine: number | null;
  previewTargetLineRevision: number;
  setPreviewTargetLine: (line: number | null) => void;
  isSearchActive: boolean;
  canOpenPreviousSearchHit: boolean;
  canOpenNextSearchHit: boolean;
  runSearchQuery: (query: string) => void;
  clearSearch: () => void;
  clearSearchNavigation: () => void;
  activateSearchHit: (hit: ProjectSearchHit) => void;
  handleOpenSearchHit: (hit: ProjectSearchHit, openFile: (entry: FsEntry) => Promise<void>) => Promise<void>;
  openPreviousSearchHit: (openFile: (entry: FsEntry) => Promise<void>) => Promise<void>;
  openNextSearchHit: (openFile: (entry: FsEntry) => Promise<void>) => Promise<void>;
}

export const useProjectExplorerSearch = ({
  client,
  projectRootPath,
}: UseProjectExplorerSearchOptions): UseProjectExplorerSearchResult => {
  const [searchQuery, setSearchQuery] = useState('');
  const [searchCaseSensitive, setSearchCaseSensitive] = useState(false);
  const [searchWholeWord, setSearchWholeWord] = useState(false);
  const [searchResults, setSearchResults] = useState<ProjectSearchHit[]>([]);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [searchTruncated, setSearchTruncated] = useState(false);
  const [activeSearchHitId, setActiveSearchHitId] = useState<string | null>(null);
  const [previewTargetLine, setPreviewTargetLine] = useState<number | null>(null);
  const [previewTargetLineRevision, setPreviewTargetLineRevision] = useState(0);

  const requestPreviewTargetLine = useCallback((line: number | null) => {
    setPreviewTargetLine(line);
    if (line !== null && line > 0) {
      setPreviewTargetLineRevision((revision) => revision + 1);
    }
  }, []);

  const clearSearchNavigation = useCallback(() => {
    setActiveSearchHitId(null);
    setPreviewTargetLine(null);
  }, []);

  const totalSearchHits = searchResults.length;
  const activeSearchHitIndex = activeSearchHitId
    ? searchResults.findIndex((hit) => buildProjectSearchHitId(hit) === activeSearchHitId)
    : -1;
  const canOpenPreviousSearchHit = totalSearchHits > 0 && activeSearchHitIndex !== 0;
  const canOpenNextSearchHit = totalSearchHits > 0 && activeSearchHitIndex !== totalSearchHits - 1;

  const clearSearch = useCallback(() => {
    setSearchQuery('');
    setSearchResults([]);
    setSearchError(null);
    setSearchTruncated(false);
    setSearchLoading(false);
    clearSearchNavigation();
  }, [clearSearchNavigation]);

  const runSearchQuery = useCallback((query: string) => {
    const keyword = query.trim();
    if (!keyword) {
      clearSearch();
      return;
    }
    clearSearchNavigation();
    setSearchError(null);
    setSearchQuery(keyword);
  }, [clearSearch, clearSearchNavigation]);

  useEffect(() => {
    clearSearch();
  }, [clearSearch, projectRootPath]);

  useEffect(() => {
    const keyword = searchQuery.trim();
    clearSearchNavigation();
    if (!projectRootPath || !keyword) {
      setSearchResults([]);
      setSearchError(null);
      setSearchTruncated(false);
      setSearchLoading(false);
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
        setSearchError(error instanceof Error ? error.message : '全文搜索失败');
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
    clearSearchNavigation,
    client,
    projectRootPath,
    searchCaseSensitive,
    searchQuery,
    searchWholeWord,
  ]);

  const handleOpenSearchHit = useCallback(async (
    hit: ProjectSearchHit,
    openFile: (entry: FsEntry) => Promise<void>,
  ) => {
    await openFile({
      name: hit.relativePath.split('/').filter(Boolean).pop() || hit.path.split(/[\\/]/).pop() || hit.path,
      path: hit.path,
      isDir: false,
      size: null,
      modifiedAt: null,
    });
    setActiveSearchHitId(buildProjectSearchHitId(hit));
    requestPreviewTargetLine(hit.line);
  }, [requestPreviewTargetLine]);

  const activateSearchHit = useCallback((hit: ProjectSearchHit) => {
    setActiveSearchHitId(buildProjectSearchHitId(hit));
    requestPreviewTargetLine(hit.line);
  }, [requestPreviewTargetLine]);

  const openRelativeSearchHit = useCallback(async (
    direction: -1 | 1,
    openFile: (entry: FsEntry) => Promise<void>,
  ) => {
    if (searchResults.length === 0) {
      return;
    }

    let nextIndex = direction > 0 ? 0 : searchResults.length - 1;
    if (activeSearchHitIndex >= 0) {
      nextIndex = Math.min(
        searchResults.length - 1,
        Math.max(0, activeSearchHitIndex + direction),
      );
      if (nextIndex === activeSearchHitIndex) {
        return;
      }
    }

    const targetHit = searchResults[nextIndex];
    if (!targetHit) {
      return;
    }
    await handleOpenSearchHit(targetHit, openFile);
  }, [activeSearchHitIndex, handleOpenSearchHit, searchResults]);

  const openPreviousSearchHit = useCallback(async (
    openFile: (entry: FsEntry) => Promise<void>,
  ) => {
    await openRelativeSearchHit(-1, openFile);
  }, [openRelativeSearchHit]);

  const openNextSearchHit = useCallback(async (
    openFile: (entry: FsEntry) => Promise<void>,
  ) => {
    await openRelativeSearchHit(1, openFile);
  }, [openRelativeSearchHit]);

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
    setPreviewTargetLine: requestPreviewTargetLine,
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
