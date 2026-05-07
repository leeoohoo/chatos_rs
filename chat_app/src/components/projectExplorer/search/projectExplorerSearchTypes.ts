import type { Dispatch, SetStateAction } from 'react';

import type { FsContentSearchResponse } from '../../../lib/api/client/types';
import type { FsEntry, ProjectSearchHit } from '../../../types';

export interface ProjectExplorerSearchApiClient {
  searchFsContent: (
    path: string,
    query: string,
    options?: { limit?: number; caseSensitive?: boolean; wholeWord?: boolean }
  ) => Promise<FsContentSearchResponse>;
}

export interface UseProjectExplorerSearchOptions {
  client: ProjectExplorerSearchApiClient;
  projectRootPath?: string | null;
}

export interface UseProjectExplorerSearchResult {
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
