// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type React from 'react';

import type {
  FsEntry,
  FsReadResult,
  Project,
  ProjectSearchHit,
} from '../../../types';

export interface UseProjectExplorerTreePanePropsParams {
  project: Project;
  treeWidth: number;
  treeScrollRef: React.MutableRefObject<HTMLDivElement | null>;
  entriesMap: Record<string, FsEntry[]>;
  expandedPaths: Set<string>;
  loadingPaths: Set<string>;
  selectedPath: string | null;
  selectedEntry: FsEntry | null;
  draggingEntryPath: string | null;
  dropTargetDirPath: string | null;
  actionLoading: boolean;
  actionReloadPath: string | null;
  actionMessage: string | null;
  actionError: string | null;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchResults: ProjectSearchHit[];
  searchLoading: boolean;
  searchError: string | null;
  searchTruncated: boolean;
  activeSearchHitId: string | null;
  activeSearchHitIndex: number;
  totalSearchHits: number;
  normalizePath: (value: string) => string;
  toExpandedKey: (path: string) => string;
  canDropToDirectory: (sourcePath: string, targetDirPath: string) => boolean;
  setSelectedPath: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedFile: React.Dispatch<React.SetStateAction<FsReadResult | null>>;
  setDraggingEntryPath: React.Dispatch<React.SetStateAction<string | null>>;
  setDropTargetDirPath: React.Dispatch<React.SetStateAction<string | null>>;
  clearDragExpandTimer: () => void;
  cancelDragExpandIfMatches: (path: string) => void;
  scheduleDragExpand: (targetDirPath: string) => void;
  clearDragAutoScroll: () => void;
  startDragAutoScroll: (velocity: number) => void;
  selectProjectRoot: () => Promise<void>;
  toggleDir: (entry: FsEntry) => Promise<void>;
  openFile: (entry: FsEntry) => Promise<void>;
  handleCreateDirectory: (path: string) => Promise<void>;
  handleCreateFile: (path: string) => Promise<void>;
  handleRefresh: () => Promise<void>;
  handleSearchQueryChange: (value: string) => void;
  handleSearchCaseSensitiveChange: React.Dispatch<React.SetStateAction<boolean>>;
  handleSearchWholeWordChange: React.Dispatch<React.SetStateAction<boolean>>;
  canOpenPreviousSearchHit: boolean;
  canOpenNextSearchHit: boolean;
  handleClearSearch: () => void;
  handleOpenSearchHit: (hit: ProjectSearchHit) => Promise<void>;
  handleOpenPreviousSearchHit: () => Promise<void>;
  handleOpenNextSearchHit: () => Promise<void>;
  handleMoveEntryByDrop: (sourcePath: string, targetDirPath: string) => Promise<void>;
  openEntryContextMenu: (event: React.MouseEvent, entry: FsEntry) => void;
  handleDragStart: (event: React.DragEvent, entry: FsEntry) => void;
  handleDragEnd: () => void;
}
