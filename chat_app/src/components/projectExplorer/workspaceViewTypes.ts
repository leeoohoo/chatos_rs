import type React from 'react';

import type {
  ChangeLogItem,
  CodeNavCapabilities,
  CodeNavDocumentSymbolsResult,
  CodeNavLocation,
  CodeNavLocationsResult,
  FsEntry,
  FsReadResult,
  Project,
  ProjectChangeSummary,
  ProjectSearchHit,
} from '../../types';
import type { MoveConflictState } from './Overlays';
import type {
  ProjectRunnerActiveTerminal,
  ProjectRunnerMember,
} from './useProjectExplorerRunState';
import type { ExplorerContextMenuState } from './useProjectExplorerState';
import type { ChangeKind } from './utils';

export interface ProjectExplorerWorkspaceTreeState {
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
  contextMenu: ExplorerContextMenuState | null;
  canConfirmCurrent: boolean;
  showOnlyChanged: boolean;
  changeSummary: ProjectChangeSummary;
  loadingSummary: boolean;
  summaryError: string | null;
  actionMessage: string | null;
  actionError: string | null;
  aggregatedChangeKindByPath: Map<string, ChangeKind>;
}

export interface ProjectExplorerWorkspaceSearchState {
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
  previewTargetLine: number | null;
  previewTargetLineRevision: number;
  canOpenPreviousSearchHit: boolean;
  canOpenNextSearchHit: boolean;
}

export interface ProjectExplorerWorkspacePreviewState {
  loadingFile: boolean;
  error: string | null;
  selectedFile: FsReadResult | null;
  selectedLog: ChangeLogItem | null;
}

export interface ProjectExplorerWorkspaceCodeNavState {
  navCapabilities: CodeNavCapabilities | null;
  navCapabilitiesLoading: boolean;
  navCapabilitiesError: string | null;
  selectedToken: string | null;
  selectedTokenLine: number | null;
  selectedTokenColumn: number | null;
  navResult: CodeNavLocationsResult | null;
  navRequestKind: 'definition' | 'references' | null;
  navLoading: boolean;
  navError: string | null;
  activeNavLocationId: string | null;
  canGoBackFromNav: boolean;
  documentSymbols: CodeNavDocumentSymbolsResult | null;
  documentSymbolsLoading: boolean;
  documentSymbolsError: string | null;
}

export interface ProjectExplorerWorkspaceRunState {
  canRunFile: (entry: FsEntry) => boolean;
  handleRunFile: (entry: FsEntry) => Promise<void>;
  runStatus: string;
  runCatalogLoading: boolean;
  runCatalogError: string | null;
  projectMembers: ProjectRunnerMember[];
  projectMembersLoading: boolean;
  projectMembersError: string | null;
  runnerScriptExists: boolean;
  runnerScriptChecking: boolean;
  runnerScriptPath: string;
  runnerStartCommand: string;
  runnerStopCommand: string;
  runnerRestartCommand: string;
  starting: boolean;
  stopping: boolean;
  restarting: boolean;
  runnerMessage: string | null;
  runnerError: string | null;
  activeRun: ProjectRunnerActiveTerminal | null;
  activeTerminalBusy: boolean;
  handleRunnerStart: () => Promise<void>;
  handleRunnerStop: () => Promise<void>;
  handleRunnerRestart: () => Promise<void>;
  refreshRunnerState: () => Promise<void>;
  handleGenerateRunnerScriptForContact: (member: ProjectRunnerMember) => Promise<void>;
}

export interface ProjectExplorerWorkspaceInteractions {
  normalizePath: (value: string) => string;
  toExpandedKey: (path: string) => string;
  canDropToDirectory: (sourcePath: string, targetDirPath: string) => boolean;
  setSelectedPath: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedFile: React.Dispatch<React.SetStateAction<FsReadResult | null>>;
  setShowOnlyChanged: React.Dispatch<React.SetStateAction<boolean>>;
  setDraggingEntryPath: React.Dispatch<React.SetStateAction<string | null>>;
  setDropTargetDirPath: React.Dispatch<React.SetStateAction<string | null>>;
  setMoveConflict: React.Dispatch<React.SetStateAction<MoveConflictState | null>>;
  setContextMenu: React.Dispatch<React.SetStateAction<ExplorerContextMenuState | null>>;
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
  handleConfirmCurrentChanges: () => Promise<void>;
  handleConfirmAllChanges: () => Promise<void>;
  handleSearchQueryChange: (value: string) => void;
  handleSearchCaseSensitiveChange: React.Dispatch<React.SetStateAction<boolean>>;
  handleSearchWholeWordChange: React.Dispatch<React.SetStateAction<boolean>>;
  handleSearchInProject: (query: string) => void;
  handleClearSearch: () => void;
  handleActivateSearchHit: (hit: ProjectSearchHit) => void;
  handleOpenSearchHit: (hit: ProjectSearchHit) => Promise<void>;
  handleOpenPreviousSearchHit: () => Promise<void>;
  handleOpenNextSearchHit: () => Promise<void>;
  handleTokenSelection: (selection: { token: string; line: number; column: number } | null) => void;
  clearTokenSelection: () => void;
  requestDefinition: () => Promise<void>;
  requestReferences: () => Promise<void>;
  handleOpenNavLocation: (location: CodeNavLocation) => Promise<void>;
  goBackFromNav: () => Promise<void>;
  handleOpenDocumentSymbol: (line: number) => void;
  handleMoveEntryByDrop: (sourcePath: string, targetDirPath: string) => Promise<void>;
  handleDownloadSelected: (entry: FsEntry) => Promise<void>;
  handleDeleteSelected: (entry: FsEntry) => Promise<void>;
}

export interface ProjectExplorerWorkspaceViewParams {
  project: Project;
  tree: ProjectExplorerWorkspaceTreeState;
  search: ProjectExplorerWorkspaceSearchState;
  preview: ProjectExplorerWorkspacePreviewState;
  codeNav: ProjectExplorerWorkspaceCodeNavState;
  run: ProjectExplorerWorkspaceRunState;
  interactions: ProjectExplorerWorkspaceInteractions;
}

export interface ProjectExplorerWorkspaceShellParams
  extends Omit<ProjectExplorerWorkspaceViewParams, 'project'> {
  project: Project | null;
}
