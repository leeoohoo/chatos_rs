import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { apiClient as globalApiClient } from '../lib/api/client';
import { useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import type {
  Project,
  FsEntry,
  FsReadResult,
  ChangeLogItem,
  ProjectChangeSummary,
} from '../types';
import { cn } from '../lib/utils';
import {
  CHANGE_KIND_PRIORITY,
  EMPTY_CHANGE_SUMMARY,
  isProjectChangeSummaryEqual,
  normalizeChangeLog,
  normalizeChangeKind,
  normalizeEntry,
  normalizeFile,
  normalizeProjectChangeSummary,
} from './projectExplorer/utils';
import type { ChangeKind } from './projectExplorer/utils';
import { ChangeLogPanel } from './projectExplorer/ChangeLogPanels';
import {
  EntryContextMenu,
  MoveConflictModal,
} from './projectExplorer/Overlays';
import type { MoveConflictState } from './projectExplorer/Overlays';
import { ProjectPreviewPane } from './projectExplorer/PreviewPane';
import { ProjectTreePane } from './projectExplorer/TreePane';
import { useProjectTreeActions } from './projectExplorer/useProjectTreeActions';

interface ProjectExplorerProps {
  project: Project | null;
  className?: string;
}

interface ExplorerContextMenuState {
  x: number;
  y: number;
  entry: FsEntry;
}

export const ProjectExplorer: React.FC<ProjectExplorerProps> = ({ project, className }) => {
  const apiClientFromContext = useChatApiClientFromContext();
  const client = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);

  const containerRef = useRef<HTMLDivElement | null>(null);
  const treeScrollRef = useRef<HTMLDivElement | null>(null);
  const resizeStartX = useRef(0);
  const resizeStartWidth = useRef(0);
  const dragExpandTimerRef = useRef<number | null>(null);
  const dragExpandPathRef = useRef<string | null>(null);
  const dragAutoScrollTimerRef = useRef<number | null>(null);
  const dragAutoScrollVelocityRef = useRef(0);
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
  const [changeLogs, setChangeLogs] = useState<ChangeLogItem[]>([]);
  const [changeSummary, setChangeSummary] = useState<ProjectChangeSummary>(EMPTY_CHANGE_SUMMARY);
  const [loadingLogs, setLoadingLogs] = useState(false);
  const [loadingSummary, setLoadingSummary] = useState(false);
  const [logsError, setLogsError] = useState<string | null>(null);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [selectedLogId, setSelectedLogId] = useState<string | null>(null);
  const [expandedReady, setExpandedReady] = useState(false);
  const [showOnlyChanged, setShowOnlyChanged] = useState(false);
  const [treeWidth, setTreeWidth] = useState(() => {
    if (typeof window === 'undefined') return 288;
    const saved = window.localStorage.getItem('project_explorer_tree_width');
    const parsed = saved ? Number(saved) : NaN;
    return Number.isFinite(parsed) ? Math.min(Math.max(parsed, 200), 640) : 288;
  });
  const [isResizing, setIsResizing] = useState(false);

  const normalizePath = useCallback((value: string) => (
    value.replace(/\\/g, '/').replace(/\/+$/, '')
  ), []);

  const rootPathNormalized = useMemo(
    () => (project?.rootPath ? normalizePath(project.rootPath) : ''),
    [project?.rootPath, normalizePath]
  );

  const toExpandedKey = useCallback((path: string) => {
    const full = normalizePath(path);
    if (!rootPathNormalized) return full;
    if (full === rootPathNormalized) return '';
    const prefix = `${rootPathNormalized}/`;
    if (full.startsWith(prefix)) {
      return full.slice(prefix.length);
    }
    return full;
  }, [rootPathNormalized, normalizePath]);

  const keyToPath = useCallback((key: string) => {
    if (!rootPathNormalized) return normalizePath(key);
    if (!key) return rootPathNormalized;
    return `${rootPathNormalized}/${key}`;
  }, [rootPathNormalized, normalizePath]);

  const loadEntries = useCallback(async (path: string) => {
    setLoadingPaths(prev => new Set(prev).add(path));
    setError(null);
    try {
      const data = await client.listFsEntries(path);
      const entries = Array.isArray(data?.entries) ? data.entries.map(normalizeEntry) : [];
      setEntriesMap(prev => ({ ...prev, [path]: entries }));
    } catch (err: any) {
      setError(err?.message || '加载目录失败');
    } finally {
      setLoadingPaths(prev => {
        const next = new Set(prev);
        next.delete(path);
        return next;
      });
    }
  }, [client]);

  const loadChangeSummary = useCallback(async (options?: { silent?: boolean }) => {
    const silent = options?.silent ?? false;
    if (!project?.id) {
      if (!silent) {
        setChangeSummary(EMPTY_CHANGE_SUMMARY);
        setSummaryError(null);
      }
      return;
    }
    if (summaryLoadingRef.current) {
      return;
    }
    summaryLoadingRef.current = true;
    if (!silent) {
      setLoadingSummary(true);
      setSummaryError(null);
    }
    try {
      const data = await client.getProjectChangeSummary(project.id);
      const nextSummary = normalizeProjectChangeSummary(data);
      setChangeSummary((prev) => (
        isProjectChangeSummaryEqual(prev, nextSummary) ? prev : nextSummary
      ));
      if (!silent) {
        setSummaryError(null);
      }
    } catch (err: any) {
      if (!silent) {
        setSummaryError(err?.message || '加载变更标记失败');
        setChangeSummary(EMPTY_CHANGE_SUMMARY);
      }
    } finally {
      if (!silent) {
        setLoadingSummary(false);
      }
      summaryLoadingRef.current = false;
    }
  }, [client, project?.id]);

  const toggleDir = useCallback(async (entry: FsEntry) => {
    if (!entry.isDir) return;
    setActionError(null);
    setSelectedPath(entry.path);
    setSelectedFile(null);
    const key = toExpandedKey(entry.path);
    setExpandedPaths(prev => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
    if (!entriesMap[entry.path]) {
      await loadEntries(entry.path);
    }
  }, [entriesMap, loadEntries, toExpandedKey]);

  const openFile = useCallback(async (entry: FsEntry) => {
    setActionError(null);
    setSelectedPath(entry.path);
    setSelectedFile(null);
    setLoadingFile(true);
    setError(null);
    try {
      const data = await client.readFsFile(entry.path);
      setSelectedFile(normalizeFile(data));
    } catch (err: any) {
      setError(err?.message || '读取文件失败');
    } finally {
      setLoadingFile(false);
    }
  }, [client]);

  const getParentPath = useCallback((value: string): string | null => {
    const normalized = normalizePath(value);
    if (!normalized) return null;
    const idx = normalized.lastIndexOf('/');
    if (idx < 0) return null;
    if (idx === 0) return '/';
    const parent = normalized.slice(0, idx);
    if (/^[A-Za-z]:$/.test(parent)) {
      return `${parent}/`;
    }
    return parent;
  }, [normalizePath]);

  const projectRootEntry = useMemo<FsEntry | null>(() => {
    if (!project?.rootPath) return null;
    return {
      name: project.name || project.rootPath,
      path: project.rootPath,
      isDir: true,
      size: null,
      modifiedAt: null,
    };
  }, [project?.name, project?.rootPath]);

  const findEntryByPath = useCallback((path: string): FsEntry | null => {
    const normalizedTarget = normalizePath(path);
    const root = project?.rootPath ? normalizePath(project.rootPath) : '';
    if (root && normalizedTarget === root) {
      return projectRootEntry;
    }
    for (const entries of Object.values(entriesMap)) {
      const found = entries.find((entry) => normalizePath(entry.path) === normalizedTarget);
      if (found) return found;
    }
    return null;
  }, [entriesMap, normalizePath, project?.rootPath, projectRootEntry]);

  const selectedEntry = useMemo<FsEntry | null>(() => {
    if (!selectedPath) return null;
    return findEntryByPath(selectedPath);
  }, [findEntryByPath, selectedPath]);

  const selectedDirPath = useMemo(
    () => (selectedEntry?.isDir ? selectedEntry.path : null),
    [selectedEntry]
  );

  const isContextRootEntry = useMemo(() => {
    if (!contextMenu?.entry.path || !project?.rootPath) return false;
    return normalizePath(contextMenu.entry.path) === normalizePath(project.rootPath);
  }, [contextMenu?.entry.path, normalizePath, project?.rootPath]);

  const actionReloadPath = useMemo(() => {
    if (!selectedEntry) return project?.rootPath || null;
    if (selectedEntry.isDir) return selectedEntry.path;
    return getParentPath(selectedEntry.path) || project?.rootPath || null;
  }, [getParentPath, project?.rootPath, selectedEntry]);

  const selectProjectRoot = useCallback(async () => {
    const root = project?.rootPath;
    if (!root) return;
    setSelectedPath(root);
    setSelectedFile(null);
    if (!entriesMap[root]) {
      await loadEntries(root);
    }
  }, [entriesMap, loadEntries, project?.rootPath]);

  const pendingMarks = useMemo(
    () => [...changeSummary.fileMarks, ...changeSummary.deletedMarks],
    [changeSummary.deletedMarks, changeSummary.fileMarks]
  );

  const hasPendingChangesForPath = useCallback((path: string | null): boolean => {
    if (!path) return false;
    const normalizedTarget = normalizePath(path);
    if (!normalizedTarget) return false;
    const prefix = `${normalizedTarget}/`;
    return pendingMarks.some((mark) => {
      const normalizedMarkPath = normalizePath(mark.path);
      return normalizedMarkPath === normalizedTarget || normalizedMarkPath.startsWith(prefix);
    });
  }, [normalizePath, pendingMarks]);

  const canConfirmCurrent = useMemo(
    () => hasPendingChangesForPath(selectedPath),
    [hasPendingChangesForPath, selectedPath]
  );

  const aggregatedChangeKindByPath = useMemo(() => {
    const map = new Map<string, ChangeKind>();
    const applyKind = (path: string, kind: ChangeKind) => {
      const prev = map.get(path);
      if (!prev || CHANGE_KIND_PRIORITY[kind] >= CHANGE_KIND_PRIORITY[prev]) {
        map.set(path, kind);
      }
    };

    for (const mark of pendingMarks) {
      const normalizedMarkPath = normalizePath(mark.path);
      if (!normalizedMarkPath) continue;
      const kind = normalizeChangeKind(mark.kind);
      applyKind(normalizedMarkPath, kind);

      let parentPath = getParentPath(normalizedMarkPath);
      while (parentPath) {
        const normalizedParent = normalizePath(parentPath);
        if (!normalizedParent) break;
        applyKind(normalizedParent, kind);
        if (rootPathNormalized && normalizedParent === rootPathNormalized) {
          break;
        }
        parentPath = getParentPath(normalizedParent);
      }
    }

    return map;
  }, [getParentPath, normalizePath, pendingMarks, rootPathNormalized]);

  const canDropToDirectory = useCallback((sourcePath: string, targetDirPath: string): boolean => {
    const normalizedSource = normalizePath(sourcePath);
    const normalizedTarget = normalizePath(targetDirPath);
    if (!normalizedSource || !normalizedTarget) return false;
    if (normalizedSource === normalizedTarget) return false;

    const targetEntry = findEntryByPath(targetDirPath);
    if (!targetEntry?.isDir) return false;

    const sourceEntry = findEntryByPath(sourcePath);
    if (!sourceEntry) return false;

    const sourceParent = getParentPath(sourcePath);
    if (sourceParent && normalizePath(sourceParent) === normalizedTarget) {
      return false;
    }

    if (sourceEntry.isDir && normalizedTarget.startsWith(`${normalizedSource}/`)) {
      return false;
    }

    return true;
  }, [findEntryByPath, getParentPath, normalizePath]);

  const clearDragExpandTimer = useCallback(() => {
    if (dragExpandTimerRef.current !== null) {
      window.clearTimeout(dragExpandTimerRef.current);
      dragExpandTimerRef.current = null;
    }
    dragExpandPathRef.current = null;
  }, []);

  const cancelDragExpandIfMatches = useCallback((path: string) => {
    const pendingPath = dragExpandPathRef.current;
    if (!pendingPath) return;
    if (normalizePath(pendingPath) !== normalizePath(path)) return;
    clearDragExpandTimer();
  }, [clearDragExpandTimer, normalizePath]);

  const scheduleDragExpand = useCallback((path: string) => {
    const normalizedPath = normalizePath(path);
    const pendingPath = dragExpandPathRef.current;
    if (pendingPath && normalizePath(pendingPath) === normalizedPath) {
      return;
    }
    clearDragExpandTimer();
    dragExpandPathRef.current = path;
    dragExpandTimerRef.current = window.setTimeout(() => {
      const key = toExpandedKey(path);
      setExpandedPaths((prev) => {
        if (prev.has(key)) return prev;
        const next = new Set(prev);
        next.add(key);
        return next;
      });
      if (!entriesMap[path] && !loadingPaths.has(path)) {
        void loadEntries(path);
      }
      dragExpandTimerRef.current = null;
      dragExpandPathRef.current = null;
    }, 500);
  }, [clearDragExpandTimer, entriesMap, loadingPaths, loadEntries, normalizePath, toExpandedKey]);

  const clearDragAutoScroll = useCallback(() => {
    if (dragAutoScrollTimerRef.current !== null) {
      window.clearInterval(dragAutoScrollTimerRef.current);
      dragAutoScrollTimerRef.current = null;
    }
    dragAutoScrollVelocityRef.current = 0;
  }, []);

  const startDragAutoScroll = useCallback((velocity: number) => {
    if (!Number.isFinite(velocity) || velocity === 0) {
      clearDragAutoScroll();
      return;
    }
    dragAutoScrollVelocityRef.current = velocity;
    if (dragAutoScrollTimerRef.current !== null) {
      return;
    }
    dragAutoScrollTimerRef.current = window.setInterval(() => {
      const container = treeScrollRef.current;
      if (!container) return;
      const nextTop = container.scrollTop + dragAutoScrollVelocityRef.current;
      const maxTop = Math.max(0, container.scrollHeight - container.clientHeight);
      container.scrollTop = Math.max(0, Math.min(maxTop, nextTop));
    }, 16);
  }, [clearDragAutoScroll]);

  const replaceExpandedPathPrefix = useCallback((sourcePath: string, movedPath: string) => {
    const normalizedSource = normalizePath(sourcePath);
    const normalizedMoved = normalizePath(movedPath);
    const sourcePrefix = `${normalizedSource}/`;
    const next = new Set<string>();
    expandedPaths.forEach((key) => {
      const full = normalizePath(keyToPath(key));
      if (full === normalizedSource || full.startsWith(sourcePrefix)) {
        const suffix = full.slice(normalizedSource.length);
        const nextPath = normalizePath(`${normalizedMoved}${suffix}`);
        next.add(toExpandedKey(nextPath));
      } else {
        next.add(key);
      }
    });
    return next;
  }, [expandedPaths, keyToPath, normalizePath, toExpandedKey]);

  const reloadTreeWithExpanded = useCallback(async (nextExpanded: Set<string>) => {
    if (!project?.rootPath) return;
    setEntriesMap({});
    await loadEntries(project.rootPath);
    const tasks = Array.from(nextExpanded)
      .filter((key) => key.length > 0)
      .map((key) => loadEntries(keyToPath(key)));
    if (tasks.length > 0) {
      await Promise.all(tasks);
    }
  }, [keyToPath, loadEntries, project?.rootPath]);

  const pruneDeletedPath = useCallback((deletedPath: string) => {
    const normalizedDeleted = normalizePath(deletedPath);
    const deletedPrefix = `${normalizedDeleted}/`;

    setEntriesMap((prev) => {
      const next: Record<string, FsEntry[]> = {};
      Object.entries(prev).forEach(([key, entries]) => {
        const normalizedKey = normalizePath(key);
        if (normalizedKey === normalizedDeleted || normalizedKey.startsWith(deletedPrefix)) {
          return;
        }
        next[key] = entries.filter((entry) => {
          const normalizedEntryPath = normalizePath(entry.path);
          return normalizedEntryPath !== normalizedDeleted && !normalizedEntryPath.startsWith(deletedPrefix);
        });
      });
      return next;
    });

    setExpandedPaths((prev) => {
      const next = new Set<string>();
      prev.forEach((key) => {
        const full = normalizePath(keyToPath(key));
        if (full !== normalizedDeleted && !full.startsWith(deletedPrefix)) {
          next.add(key);
        }
      });
      return next;
    });
  }, [keyToPath, normalizePath]);

  const {
    handleCreateDirectory,
    handleCreateFile,
    handleDeleteSelected,
    handleDownloadSelected,
    handleRefresh,
    handleConfirmCurrentChanges,
    handleConfirmAllChanges,
    handleMoveEntryByDrop,
    handleMoveConflictCancel,
    handleMoveConflictOverwrite,
    handleMoveConflictRename,
  } = useProjectTreeActions({
    client,
    selectedDirPath,
    selectedEntry,
    selectedFilePath: selectedFile?.path || null,
    selectedPath,
    projectRootPath: project?.rootPath,
    projectId: project?.id,
    actionReloadPath,
    normalizePath,
    getParentPath,
    toExpandedKey,
    loadEntries,
    loadChangeSummary,
    hasPendingChangesForPath,
    pruneDeletedPath,
    replaceExpandedPathPrefix,
    reloadTreeWithExpanded,
    canDropToDirectory,
    findEntryByPath,
    clearDragExpandTimer,
    clearDragAutoScroll,
    setExpandedPaths,
    setSelectedPath,
    setSelectedFile,
    setActionLoading,
    setActionError,
    setActionMessage,
    setMoveConflict,
    openFile,
  });

  const handleDragStart = useCallback((event: React.DragEvent, entry: FsEntry) => {
    if (!entry.path) return;
    clearDragExpandTimer();
    clearDragAutoScroll();
    setDraggingEntryPath(entry.path);
    setDropTargetDirPath(null);
    setMoveConflict(null);
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('text/plain', entry.path);
  }, [clearDragAutoScroll, clearDragExpandTimer]);

  const handleDragEnd = useCallback(() => {
    clearDragExpandTimer();
    clearDragAutoScroll();
    setDraggingEntryPath(null);
    setDropTargetDirPath(null);
  }, [clearDragAutoScroll, clearDragExpandTimer]);

  const openEntryContextMenu = useCallback((event: React.MouseEvent, entry: FsEntry) => {
    event.preventDefault();
    event.stopPropagation();
    setSelectedPath(entry.path);
    if (entry.isDir) {
      setSelectedFile(null);
    }
    setContextMenu({
      x: event.clientX,
      y: event.clientY,
      entry,
    });
  }, []);

  const contextMenuStyle = useMemo(() => {
    if (!contextMenu) return undefined;
    const maxX = typeof window !== 'undefined' ? window.innerWidth - 220 : contextMenu.x;
    const maxY = typeof window !== 'undefined' ? window.innerHeight - 240 : contextMenu.y;
    return {
      left: `${Math.max(8, Math.min(contextMenu.x, maxX))}px`,
      top: `${Math.max(8, Math.min(contextMenu.y, maxY))}px`,
    };
  }, [contextMenu]);

  useEffect(() => {
    if (!project?.rootPath) {
      clearDragExpandTimer();
      clearDragAutoScroll();
      setEntriesMap({});
      setExpandedPaths(new Set());
      setSelectedPath(null);
      setSelectedFile(null);
      setActionMessage(null);
      setActionError(null);
      setActionLoading(false);
      setContextMenu(null);
      setMoveConflict(null);
      setDraggingEntryPath(null);
      setDropTargetDirPath(null);
      setChangeLogs([]);
      setChangeSummary(EMPTY_CHANGE_SUMMARY);
      setLogsError(null);
      setSummaryError(null);
      setLoadingSummary(false);
      summaryLoadingRef.current = false;
      setSelectedLogId(null);
      setExpandedReady(false);
      return;
    }
    const root = project.rootPath;
    clearDragExpandTimer();
    clearDragAutoScroll();
    setEntriesMap({});
    const saved = project.id ? localStorage.getItem(`project_explorer_expanded_${project.id}`) : null;
    let nextExpanded = new Set<string>();
    if (saved) {
      try {
        const parsed = JSON.parse(saved);
        if (Array.isArray(parsed)) {
          nextExpanded = new Set(
            parsed
              .filter((p) => typeof p === 'string')
              .map((p) => toExpandedKey(p))
          );
        }
      } catch {
        nextExpanded = new Set();
      }
    }
    setExpandedPaths(nextExpanded);
    setExpandedReady(true);
    setSelectedPath(root);
    setSelectedFile(null);
    setActionMessage(null);
    setActionError(null);
    setActionLoading(false);
    setContextMenu(null);
    setMoveConflict(null);
    setDraggingEntryPath(null);
    setDropTargetDirPath(null);
    setChangeLogs([]);
    setChangeSummary(EMPTY_CHANGE_SUMMARY);
    setLogsError(null);
    setSummaryError(null);
    setSelectedLogId(null);
    loadEntries(root);
    void loadChangeSummary();
    nextExpanded.forEach((p) => {
      if (!p) return;
      const full = keyToPath(p);
      if (full !== root) loadEntries(full);
    });
  }, [clearDragAutoScroll, clearDragExpandTimer, project?.id, project?.rootPath, loadChangeSummary, loadEntries, keyToPath, toExpandedKey]);

  useEffect(() => {
    if (!expandedReady || !project?.id || !project?.rootPath) return;
    const next = Array.from(expandedPaths);
    localStorage.setItem(`project_explorer_expanded_${project.id}`, JSON.stringify(next));
  }, [expandedPaths, expandedReady, project?.id, project?.rootPath]);

  useEffect(() => {
    if (!project?.id) return undefined;
    const timer = window.setInterval(() => {
      void loadChangeSummary({ silent: true });
    }, 6000);
    return () => {
      window.clearInterval(timer);
    };
  }, [loadChangeSummary, project?.id]);

  useEffect(() => {
    if (!project?.id) {
      setShowOnlyChanged(false);
      return;
    }
    if (typeof window === 'undefined') return;
    const saved = window.localStorage.getItem(`project_explorer_only_changed_${project.id}`);
    setShowOnlyChanged(saved === '1');
  }, [project?.id]);

  useEffect(() => {
    if (!project?.id || typeof window === 'undefined') return;
    window.localStorage.setItem(
      `project_explorer_only_changed_${project.id}`,
      showOnlyChanged ? '1' : '0',
    );
  }, [project?.id, showOnlyChanged]);

  useEffect(() => {
    if (!contextMenu) return undefined;
    const closeMenu = () => setContextMenu(null);
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        closeMenu();
      }
    };

    window.addEventListener('click', closeMenu);
    window.addEventListener('resize', closeMenu);
    window.addEventListener('scroll', closeMenu, true);
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('click', closeMenu);
      window.removeEventListener('resize', closeMenu);
      window.removeEventListener('scroll', closeMenu, true);
      window.removeEventListener('keydown', onKeyDown);
    };
  }, [contextMenu]);

  useEffect(() => (() => {
    clearDragExpandTimer();
    clearDragAutoScroll();
  }), [clearDragAutoScroll, clearDragExpandTimer]);

  useEffect(() => {
    if (!isResizing) return;
    const handleMove = (event: MouseEvent) => {
      const delta = event.clientX - resizeStartX.current;
      const next = Math.min(Math.max(resizeStartWidth.current + delta, 200), 640);
      setTreeWidth(next);
    };
    const handleUp = () => {
      setIsResizing(false);
    };
    window.addEventListener('mousemove', handleMove);
    window.addEventListener('mouseup', handleUp);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    return () => {
      window.removeEventListener('mousemove', handleMove);
      window.removeEventListener('mouseup', handleUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, [isResizing]);

  useEffect(() => {
    localStorage.setItem('project_explorer_tree_width', String(treeWidth));
  }, [treeWidth]);

  useEffect(() => {
    const logPath = selectedFile?.path || selectedPath;
    if (!project?.id || !logPath) {
      setChangeLogs([]);
      setLogsError(null);
      setSelectedLogId(null);
      return;
    }
    let cancelled = false;
    const loadLogs = async () => {
      setLoadingLogs(true);
      setLogsError(null);
      try {
        const list = await client.listProjectChangeLogs(project.id, { path: logPath, limit: 100 });
        if (!cancelled) {
          const normalized = Array.isArray(list) ? list.map(normalizeChangeLog) : [];
          setChangeLogs(normalized);
        }
      } catch (err: any) {
        if (!cancelled) {
          setLogsError(err?.message || '加载变更记录失败');
          setChangeLogs([]);
          setSelectedLogId(null);
        }
      } finally {
        if (!cancelled) {
          setLoadingLogs(false);
        }
      }
    };
    loadLogs();
    return () => { cancelled = true; };
  }, [client, project?.id, selectedFile?.path, selectedPath]);

  useEffect(() => {
    if (selectedLogId && !changeLogs.find(log => log.id === selectedLogId)) {
      setSelectedLogId(null);
    }
  }, [changeLogs, selectedLogId]);

  const selectedLog = useMemo(
    () => (selectedLogId ? changeLogs.find(log => log.id === selectedLogId) || null : null),
    [changeLogs, selectedLogId]
  );

  if (!project) {
    return (
      <div className={cn('flex items-center justify-center h-full text-muted-foreground', className)}>
        请选择一个项目查看文件
      </div>
    );
  }

  return (
    <div ref={containerRef} className={cn('flex h-full overflow-hidden', className)}>
      <ProjectTreePane
        project={project}
        treeWidth={treeWidth}
        treeScrollRef={treeScrollRef}
        entriesMap={entriesMap}
        expandedPaths={expandedPaths}
        loadingPaths={loadingPaths}
        selectedPath={selectedPath}
        selectedEntry={selectedEntry}
        draggingEntryPath={draggingEntryPath}
        dropTargetDirPath={dropTargetDirPath}
        actionLoading={actionLoading}
        actionReloadPath={actionReloadPath}
        canConfirmCurrent={canConfirmCurrent}
        showOnlyChanged={showOnlyChanged}
        changeSummary={changeSummary}
        loadingSummary={loadingSummary}
        summaryError={summaryError}
        actionMessage={actionMessage}
        actionError={actionError}
        aggregatedChangeKindByPath={aggregatedChangeKindByPath}
        normalizePath={normalizePath}
        toExpandedKey={toExpandedKey}
        canDropToDirectory={canDropToDirectory}
        onSelectProjectRoot={() => {
          void selectProjectRoot();
        }}
        onToggleShowOnlyChanged={() => {
          setShowOnlyChanged((prev) => !prev);
        }}
        onCreateDirectoryAtRoot={() => {
          void handleCreateDirectory(project.rootPath);
        }}
        onCreateFileAtRoot={() => {
          void handleCreateFile(project.rootPath);
        }}
        onRefresh={() => {
          void handleRefresh();
        }}
        onConfirmCurrent={() => {
          void handleConfirmCurrentChanges();
        }}
        onConfirmAll={() => {
          void handleConfirmAllChanges();
        }}
        onOpenContextMenu={openEntryContextMenu}
        onSelectDeletedPath={(path) => {
          setSelectedPath(path);
          setSelectedFile(null);
        }}
        onToggleDir={(entry) => {
          void toggleDir(entry);
        }}
        onOpenFile={(entry) => {
          void openFile(entry);
        }}
        onDragStart={handleDragStart}
        onDragEnd={handleDragEnd}
        onSetDropTargetDirPath={setDropTargetDirPath}
        onSetDraggingEntryPath={setDraggingEntryPath}
        onMoveEntryByDrop={(sourcePath, targetDirPath) => {
          void handleMoveEntryByDrop(sourcePath, targetDirPath);
        }}
        onScheduleDragExpand={scheduleDragExpand}
        onCancelDragExpandIfMatches={cancelDragExpandIfMatches}
        onClearDragExpandTimer={clearDragExpandTimer}
        onStartDragAutoScroll={startDragAutoScroll}
        onClearDragAutoScroll={clearDragAutoScroll}
      />
      <div
        className={cn('w-1 cursor-col-resize bg-border/60 hover:bg-border', isResizing && 'bg-border')}
        onMouseDown={(event) => {
          resizeStartX.current = event.clientX;
          resizeStartWidth.current = treeWidth;
          setIsResizing(true);
        }}
      />
      <div className="flex-1 flex overflow-hidden">
        <ProjectPreviewPane
          selectedFile={selectedFile}
          selectedPath={selectedPath}
          selectedEntry={selectedEntry}
          loadingFile={loadingFile}
          error={error}
          selectedLog={selectedLog}
        />
        {(loadingLogs || logsError || changeLogs.length > 0) && (
          <div className="w-72 border-l border-border bg-card/60 flex flex-col overflow-hidden">
            <div className="px-4 py-2 text-xs font-medium text-foreground border-b border-border">变更记录</div>
            <div className="flex-1 min-h-0 overflow-auto">
              <ChangeLogPanel
                selectedPath={selectedPath}
                loadingLogs={loadingLogs}
                logsError={logsError}
                changeLogs={changeLogs}
                selectedLogId={selectedLogId}
                onToggleLog={(logId) => {
                  setSelectedLogId((prev) => (prev === logId ? null : logId));
                }}
              />
            </div>
          </div>
        )}
      </div>
      <MoveConflictModal
        moveConflict={moveConflict}
        actionLoading={actionLoading}
        onCancel={handleMoveConflictCancel}
        onRenameChange={(value) => {
          setMoveConflict((prev) => (prev ? { ...prev, renameTo: value } : prev));
        }}
        onOverwrite={() => {
          void handleMoveConflictOverwrite(moveConflict);
        }}
        onRename={() => {
          void handleMoveConflictRename(moveConflict);
        }}
      />
      <EntryContextMenu
        contextMenu={contextMenu}
        contextMenuStyle={contextMenuStyle}
        isContextRootEntry={isContextRootEntry}
        onCreateDirectory={(path) => {
          setContextMenu(null);
          void handleCreateDirectory(path);
        }}
        onCreateFile={(path) => {
          setContextMenu(null);
          void handleCreateFile(path);
        }}
        onDownload={(entry) => {
          setContextMenu(null);
          void handleDownloadSelected(entry);
        }}
        onDelete={(entry) => {
          setContextMenu(null);
          void handleDeleteSelected(entry);
        }}
      />
    </div>
  );
};

export default ProjectExplorer;
