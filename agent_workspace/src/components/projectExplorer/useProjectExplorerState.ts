import { useRef, useState } from 'react';

import type { FsEntry, FsReadResult, ProjectChangeSummary } from '../../types';
import type { MoveConflictState } from './Overlays';
import type { WorkspaceTab } from './WorkspaceTabs';
import { EMPTY_CHANGE_SUMMARY } from './utils';

export interface ExplorerContextMenuState {
  x: number;
  y: number;
  entry: FsEntry;
}

export const useProjectExplorerState = () => {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const treeScrollRef = useRef<HTMLDivElement | null>(null);
  const resizeStartX = useRef(0);
  const resizeStartWidth = useRef(0);
  const summaryLoadingRef = useRef(false);

  const [entriesMap, setEntriesMap] = useState<Record<string, FsEntry[]>>({});
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [loadingPaths, setLoadingPaths] = useState<Set<string>>(new Set());
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [selectedFile, setSelectedFile] = useState<FsReadResult | null>(null);
  const [loadingFile, setLoadingFile] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState(false);
  const [contextMenu, setContextMenu] = useState<ExplorerContextMenuState | null>(null);
  const [moveConflict, setMoveConflict] = useState<MoveConflictState | null>(null);
  const [draggingEntryPath, setDraggingEntryPath] = useState<string | null>(null);
  const [dropTargetDirPath, setDropTargetDirPath] = useState<string | null>(null);
  const [changeSummary, setChangeSummary] = useState<ProjectChangeSummary>(EMPTY_CHANGE_SUMMARY);
  const [loadingSummary, setLoadingSummary] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [expandedReady, setExpandedReady] = useState(false);
  const [showOnlyChanged, setShowOnlyChanged] = useState(false);
  const [workspaceTab, setWorkspaceTab] = useState<WorkspaceTab>('files');
  const [treeWidth, setTreeWidth] = useState(() => {
    if (typeof window === 'undefined') return 288;
    const saved = window.localStorage.getItem('project_explorer_tree_width');
    const parsed = saved ? Number(saved) : NaN;
    return Number.isFinite(parsed) ? Math.min(Math.max(parsed, 200), 640) : 288;
  });
  const [isResizing, setIsResizing] = useState(false);

  return {
    containerRef,
    treeScrollRef,
    resizeStartX,
    resizeStartWidth,
    summaryLoadingRef,
    entriesMap,
    setEntriesMap,
    expandedPaths,
    setExpandedPaths,
    loadingPaths,
    setLoadingPaths,
    selectedPath,
    setSelectedPath,
    selectedFile,
    setSelectedFile,
    loadingFile,
    setLoadingFile,
    error,
    setError,
    actionMessage,
    setActionMessage,
    actionError,
    setActionError,
    actionLoading,
    setActionLoading,
    contextMenu,
    setContextMenu,
    moveConflict,
    setMoveConflict,
    draggingEntryPath,
    setDraggingEntryPath,
    dropTargetDirPath,
    setDropTargetDirPath,
    changeSummary,
    setChangeSummary,
    loadingSummary,
    setLoadingSummary,
    summaryError,
    setSummaryError,
    expandedReady,
    setExpandedReady,
    showOnlyChanged,
    setShowOnlyChanged,
    workspaceTab,
    setWorkspaceTab,
    treeWidth,
    setTreeWidth,
    isResizing,
    setIsResizing,
  };
};
