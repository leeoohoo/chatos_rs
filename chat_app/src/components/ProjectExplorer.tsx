import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import hljs from 'highlight.js';
import { apiClient as globalApiClient } from '../lib/api/client';
import { useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import type {
  Project,
  FsEntry,
  FsReadResult,
  ChangeLogItem,
  ProjectChangeSummary,
  ProjectChangeMark,
} from '../types';
import { cn, formatFileSize } from '../lib/utils';

interface ProjectExplorerProps {
  project: Project | null;
  className?: string;
}

interface ExplorerContextMenuState {
  x: number;
  y: number;
  entry: FsEntry;
}

interface MoveConflictState {
  sourcePath: string;
  targetDirPath: string;
  sourceName: string;
  renameTo: string;
}

type ChangeKind = 'create' | 'edit' | 'delete';

const normalizeEntry = (raw: any): FsEntry => ({
  name: raw?.name ?? '',
  path: raw?.path ?? '',
  isDir: raw?.is_dir ?? raw?.isDir ?? false,
  size: raw?.size ?? null,
  modifiedAt: raw?.modified_at ?? raw?.modifiedAt ?? null,
});

const normalizeFile = (raw: any): FsReadResult => ({
  path: raw?.path ?? '',
  name: raw?.name ?? '',
  size: raw?.size ?? 0,
  contentType: raw?.content_type ?? raw?.contentType ?? 'application/octet-stream',
  isBinary: raw?.is_binary ?? raw?.isBinary ?? false,
  modifiedAt: raw?.modified_at ?? raw?.modifiedAt ?? null,
  content: raw?.content ?? '',
});

const normalizeChangeLog = (raw: any): ChangeLogItem => ({
  id: raw?.id ?? '',
  serverName: raw?.server_name ?? raw?.serverName ?? '',
  path: raw?.path ?? '',
  action: raw?.action ?? '',
  changeKind: raw?.change_kind ?? raw?.changeKind ?? (raw?.action === 'delete' ? 'delete' : 'edit'),
  bytes: raw?.bytes ?? 0,
  sha256: raw?.sha256 ?? null,
  diff: raw?.diff ?? null,
  sessionId: raw?.session_id ?? raw?.sessionId ?? null,
  runId: raw?.run_id ?? raw?.runId ?? null,
  confirmed: Boolean(raw?.confirmed),
  confirmedAt: raw?.confirmed_at ?? raw?.confirmedAt ?? null,
  confirmedBy: raw?.confirmed_by ?? raw?.confirmedBy ?? null,
  createdAt: raw?.created_at ?? raw?.createdAt ?? '',
  sessionTitle: raw?.session_title ?? raw?.sessionTitle ?? null,
});

const normalizeChangeKind = (value: any): ChangeKind => {
  const kind = String(value ?? '').trim().toLowerCase();
  if (kind === 'create') return 'create';
  if (kind === 'delete') return 'delete';
  return 'edit';
};

const normalizeProjectChangeMark = (raw: any): ProjectChangeMark => ({
  path: raw?.path ?? '',
  relativePath: raw?.relative_path ?? raw?.relativePath ?? '',
  kind: normalizeChangeKind(raw?.kind),
  lastChangeId: raw?.last_change_id ?? raw?.lastChangeId ?? '',
  updatedAt: raw?.updated_at ?? raw?.updatedAt ?? '',
});

const areChangeMarksEqual = (left: ProjectChangeMark[], right: ProjectChangeMark[]): boolean => {
  if (left.length !== right.length) return false;
  for (let i = 0; i < left.length; i += 1) {
    const a = left[i];
    const b = right[i];
    if (
      a.path !== b.path ||
      a.relativePath !== b.relativePath ||
      a.kind !== b.kind ||
      a.lastChangeId !== b.lastChangeId ||
      a.updatedAt !== b.updatedAt
    ) {
      return false;
    }
  }
  return true;
};

const EMPTY_CHANGE_SUMMARY: ProjectChangeSummary = {
  fileMarks: [],
  deletedMarks: [],
  counts: {
    create: 0,
    edit: 0,
    delete: 0,
    total: 0,
  },
};

const normalizeProjectChangeSummary = (raw: any): ProjectChangeSummary => {
  const fileMarks = Array.isArray(raw?.file_marks ?? raw?.fileMarks)
    ? (raw?.file_marks ?? raw?.fileMarks).map(normalizeProjectChangeMark)
    : [];
  const deletedMarks = Array.isArray(raw?.deleted_marks ?? raw?.deletedMarks)
    ? (raw?.deleted_marks ?? raw?.deletedMarks).map(normalizeProjectChangeMark)
    : [];
  const countsRaw = raw?.counts ?? {};
  const create = Number(countsRaw?.create ?? 0);
  const edit = Number(countsRaw?.edit ?? 0);
  const del = Number(countsRaw?.delete ?? 0);
  const total = Number(countsRaw?.total ?? create + edit + del);
  return {
    fileMarks,
    deletedMarks,
    counts: {
      create: Number.isFinite(create) ? create : 0,
      edit: Number.isFinite(edit) ? edit : 0,
      delete: Number.isFinite(del) ? del : 0,
      total: Number.isFinite(total) ? total : 0,
    },
  };
};

const isProjectChangeSummaryEqual = (
  left: ProjectChangeSummary,
  right: ProjectChangeSummary
): boolean => {
  if (
    left.counts.create !== right.counts.create ||
    left.counts.edit !== right.counts.edit ||
    left.counts.delete !== right.counts.delete ||
    left.counts.total !== right.counts.total
  ) {
    return false;
  }
  return (
    areChangeMarksEqual(left.fileMarks, right.fileMarks)
    && areChangeMarksEqual(left.deletedMarks, right.deletedMarks)
  );
};

const CHANGE_KIND_COLOR_CLASS: Record<ChangeKind, string> = {
  create: 'bg-emerald-500',
  edit: 'bg-amber-500',
  delete: 'bg-rose-500',
};

const CHANGE_KIND_TEXT_CLASS: Record<ChangeKind, string> = {
  create: 'text-emerald-600 dark:text-emerald-400',
  edit: 'text-amber-600 dark:text-amber-400',
  delete: 'text-rose-600 dark:text-rose-400',
};

const CHANGE_KIND_ROW_CLASS: Record<ChangeKind, string> = {
  create: 'border-l-2 border-emerald-500 bg-emerald-500/10',
  edit: 'border-l-2 border-amber-500 bg-amber-500/10',
  delete: 'border-l-2 border-rose-500 bg-rose-500/10',
};

const CHANGE_KIND_LABEL: Record<ChangeKind, string> = {
  create: '新增',
  edit: '编辑',
  delete: '删除',
};

const CHANGE_KIND_PRIORITY: Record<ChangeKind, number> = {
  create: 2,
  edit: 1,
  delete: 3,
};

const EXT_LANGUAGE_MAP: Record<string, string> = {
  rs: 'rust',
  toml: 'toml',
  lock: 'toml',
  md: 'markdown',
  txt: 'plaintext',
  json: 'json',
  yml: 'yaml',
  yaml: 'yaml',
  xml: 'xml',
  html: 'xml',
  htm: 'xml',
  vue: 'vue',
  svelte: 'svelte',
  astro: 'astro',
  css: 'css',
  scss: 'scss',
  less: 'less',
  js: 'javascript',
  jsx: 'javascript',
  ts: 'typescript',
  tsx: 'typescript',
  mjs: 'javascript',
  cjs: 'javascript',
  py: 'python',
  go: 'go',
  java: 'java',
  kt: 'kotlin',
  swift: 'swift',
  c: 'c',
  cc: 'cpp',
  cpp: 'cpp',
  h: 'cpp',
  hpp: 'cpp',
  cs: 'csharp',
  php: 'php',
  rb: 'ruby',
  sh: 'bash',
  bash: 'bash',
  zsh: 'bash',
  ps1: 'powershell',
  bat: 'dos',
  sql: 'sql',
  ini: 'ini',
  conf: 'ini',
  env: 'ini',
  log: 'plaintext',
  gradle: 'gradle',
  properties: 'ini',
  cfg: 'ini',
  proto: 'protobuf',
  graphql: 'graphql',
  dart: 'dart',
  lua: 'lua',
  r: 'r',
  m: 'objectivec',
  mm: 'objectivec',
  scala: 'scala',
  cmake: 'cmake',
  make: 'makefile',
  dockerfile: 'dockerfile',
};

const getHighlightLanguage = (filename: string): string | null => {
  const lower = filename.toLowerCase();
  if (lower === 'dockerfile') return hljs.getLanguage('dockerfile') ? 'dockerfile' : null;
  if (lower === 'makefile') return hljs.getLanguage('makefile') ? 'makefile' : null;
  if (lower === 'cmakelists.txt') return hljs.getLanguage('cmake') ? 'cmake' : null;
  const parts = lower.split('.');
  if (parts.length < 2) return null;
  const ext = parts[parts.length - 1];
  const lang = EXT_LANGUAGE_MAP[ext];
  if (!lang) return null;
  return hljs.getLanguage(lang) ? lang : null;
};

const escapeHtml = (value: string) => (
  value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
);

const isValidEntryName = (name: string): boolean => (
  name !== '.' &&
  name !== '..' &&
  !name.includes('/') &&
  !name.includes('\\') &&
  !name.includes('\0')
);

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

  const isRootSelected = useMemo(() => {
    if (!selectedEntry?.path || !project?.rootPath) return false;
    return normalizePath(selectedEntry.path) === normalizePath(project.rootPath);
  }, [normalizePath, project?.rootPath, selectedEntry?.path]);

  const isContextRootEntry = useMemo(() => {
    if (!contextMenu?.entry.path || !project?.rootPath) return false;
    return normalizePath(contextMenu.entry.path) === normalizePath(project.rootPath);
  }, [contextMenu?.entry.path, normalizePath, project?.rootPath]);

  const actionReloadPath = useMemo(() => {
    if (!selectedEntry) return project?.rootPath || null;
    if (selectedEntry.isDir) return selectedEntry.path;
    return getParentPath(selectedEntry.path) || project?.rootPath || null;
  }, [getParentPath, project?.rootPath, selectedEntry]);

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

  const handleCreateDirectory = useCallback(async (dirPathOverride?: string) => {
    const targetDirPath = dirPathOverride || selectedDirPath;
    if (!targetDirPath) {
      setActionError('请先选择一个目录');
      return;
    }
    const rawName = window.prompt('请输入新目录名称');
    if (rawName === null) return;
    const name = rawName.trim();
    if (!name) {
      setActionError('目录名称不能为空');
      return;
    }
    if (!isValidEntryName(name)) {
      setActionError('目录名称不合法');
      return;
    }

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    setMoveConflict(null);
    try {
      await client.createFsDirectory(targetDirPath, name);
      setExpandedPaths((prev) => {
        const next = new Set(prev);
        next.add(toExpandedKey(targetDirPath));
        return next;
      });
      await loadEntries(targetDirPath);
      setActionMessage(`已创建目录：${name}`);
    } catch (err: any) {
      setActionError(err?.message || '创建目录失败');
    } finally {
      setActionLoading(false);
    }
  }, [client, loadEntries, selectedDirPath, toExpandedKey]);

  const handleCreateFile = useCallback(async (dirPathOverride?: string) => {
    const targetDirPath = dirPathOverride || selectedDirPath;
    if (!targetDirPath) {
      setActionError('请先选择一个目录');
      return;
    }
    const rawName = window.prompt('请输入新文件名称');
    if (rawName === null) return;
    const name = rawName.trim();
    if (!name) {
      setActionError('文件名称不能为空');
      return;
    }
    if (!isValidEntryName(name)) {
      setActionError('文件名称不合法');
      return;
    }

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const data = await client.createFsFile(targetDirPath, name, '');
      const createdPath = typeof data?.path === 'string' ? data.path.trim() : '';
      setExpandedPaths((prev) => {
        const next = new Set(prev);
        next.add(toExpandedKey(targetDirPath));
        return next;
      });
      await loadEntries(targetDirPath);
      setActionMessage(`已创建文件：${name}`);
      if (createdPath) {
        await openFile({
          name,
          path: createdPath,
          isDir: false,
          size: 0,
          modifiedAt: null,
        });
      }
    } catch (err: any) {
      setActionError(err?.message || '创建文件失败');
    } finally {
      setActionLoading(false);
    }
  }, [client, loadEntries, openFile, selectedDirPath, toExpandedKey]);

  const handleDeleteSelected = useCallback(async (entryOverride?: FsEntry) => {
    const targetEntry = entryOverride || selectedEntry;
    if (!targetEntry) {
      setActionError('请先选择要删除的文件或目录');
      return;
    }
    const targetIsRoot = !!project?.rootPath
      && normalizePath(targetEntry.path) === normalizePath(project.rootPath);
    if (targetIsRoot) {
      setActionError('不支持删除项目根目录');
      return;
    }

    const confirmed = window.confirm(
      targetEntry.isDir
        ? `确认删除目录 "${targetEntry.name}" 吗？将递归删除其全部内容。`
        : `确认删除文件 "${targetEntry.name}" 吗？`
    );
    if (!confirmed) return;

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      await client.deleteFsEntry(targetEntry.path, targetEntry.isDir);
      pruneDeletedPath(targetEntry.path);
      if (selectedFile?.path && normalizePath(selectedFile.path) === normalizePath(targetEntry.path)) {
        setSelectedFile(null);
      }

      const fallbackPath = getParentPath(targetEntry.path) || project?.rootPath || null;
      setSelectedPath(fallbackPath);
      if (fallbackPath) {
        await loadEntries(fallbackPath);
      }
      setActionMessage(`已删除：${targetEntry.name}`);
    } catch (err: any) {
      setActionError(err?.message || '删除失败');
    } finally {
      setActionLoading(false);
    }
  }, [
    client,
    getParentPath,
    loadEntries,
    normalizePath,
    project?.rootPath,
    pruneDeletedPath,
    selectedEntry,
    selectedFile?.path,
  ]);

  const handleDownloadSelected = useCallback(async (entryOverride?: FsEntry) => {
    const targetEntry = entryOverride || selectedEntry;
    if (!targetEntry) {
      setActionError('请先选择要下载的文件或目录');
      return;
    }
    if (typeof document === 'undefined') {
      setActionError('当前环境不支持下载');
      return;
    }

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const { blob, filename } = await client.downloadFsEntry(targetEntry.path);
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = filename || targetEntry.name || 'download';
      anchor.style.display = 'none';
      document.body.appendChild(anchor);
      anchor.click();
      document.body.removeChild(anchor);
      URL.revokeObjectURL(url);
      setActionMessage(`开始下载：${anchor.download}`);
    } catch (err: any) {
      setActionError(err?.message || '下载失败');
    } finally {
      setActionLoading(false);
    }
  }, [client, selectedEntry]);

  const handleRefresh = useCallback(async () => {
    if (!actionReloadPath) return;
    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      await loadEntries(actionReloadPath);
      await loadChangeSummary();
      setActionMessage('目录已刷新');
    } catch (err: any) {
      setActionError(err?.message || '刷新失败');
    } finally {
      setActionLoading(false);
    }
  }, [actionReloadPath, loadChangeSummary, loadEntries]);

  const handleConfirmCurrentChanges = useCallback(async () => {
    if (!project?.id) return;
    if (!selectedPath) {
      setActionError('请先选择要确认的文件或目录');
      return;
    }
    if (!hasPendingChangesForPath(selectedPath)) {
      setActionError('当前项没有未确认变更');
      return;
    }

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const result = await client.confirmProjectChanges(project.id, {
        mode: 'paths',
        paths: [selectedPath],
      });
      await loadChangeSummary();
      const confirmed = Number(result?.confirmed ?? 0);
      if (Number.isFinite(confirmed) && confirmed > 0) {
        setActionMessage(`已确认当前项变更（${confirmed} 条）`);
      } else {
        setActionMessage('当前项没有可确认的变更');
      }
    } catch (err: any) {
      setActionError(err?.message || '确认当前项变更失败');
    } finally {
      setActionLoading(false);
    }
  }, [client, hasPendingChangesForPath, loadChangeSummary, project?.id, selectedPath]);

  const handleConfirmAllChanges = useCallback(async () => {
    if (!project?.id) return;

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const result = await client.confirmProjectChanges(project.id, { mode: 'all' });
      await loadChangeSummary();
      const confirmed = Number(result?.confirmed ?? 0);
      if (Number.isFinite(confirmed) && confirmed > 0) {
        setActionMessage(`已确认全部变更（${confirmed} 条）`);
      } else {
        setActionMessage('暂无可确认的变更');
      }
    } catch (err: any) {
      setActionError(err?.message || '确认全部变更失败');
    } finally {
      setActionLoading(false);
    }
  }, [client, loadChangeSummary, project?.id]);

  const applyMoveResult = useCallback(async (
    sourcePath: string,
    targetDirPath: string,
    result: any,
    movedLabel: string
  ) => {
    const movedPath = typeof result?.to_path === 'string' ? result.to_path : '';
    if (!movedPath) {
      throw new Error('移动成功，但返回路径为空');
    }
    const nextExpanded = replaceExpandedPathPrefix(sourcePath, movedPath);
    nextExpanded.add(toExpandedKey(targetDirPath));
    setExpandedPaths(nextExpanded);
    setSelectedPath(movedPath);
    setSelectedFile(null);
    await reloadTreeWithExpanded(nextExpanded);
    setActionMessage(`已移动：${movedLabel}`);
  }, [reloadTreeWithExpanded, replaceExpandedPathPrefix, toExpandedKey]);

  const executeMoveEntry = useCallback(async (
    sourcePath: string,
    targetDirPath: string,
    movedLabel: string,
    options?: { targetName?: string; replaceExisting?: boolean }
  ) => {
    const result = await client.moveFsEntry(sourcePath, targetDirPath, options);
    await applyMoveResult(sourcePath, targetDirPath, result, movedLabel);
    return result;
  }, [applyMoveResult, client]);

  const handleMoveEntryByDrop = useCallback(async (sourcePath: string, targetDirPath: string) => {
    clearDragExpandTimer();
    clearDragAutoScroll();
    if (!canDropToDirectory(sourcePath, targetDirPath)) return;
    const sourceEntry = findEntryByPath(sourcePath);
    if (!sourceEntry) {
      setActionError('拖拽源文件不存在');
      return;
    }

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      try {
        await executeMoveEntry(sourcePath, targetDirPath, sourceEntry.name);
      } catch (err: any) {
        const message = String(err?.message || '');
        if (!message.includes('已存在同名')) {
          throw err;
        }
        setMoveConflict({
          sourcePath,
          targetDirPath,
          sourceName: sourceEntry.name,
          renameTo: `${sourceEntry.name}_copy`,
        });
        setActionMessage('目标已存在同名项，请选择处理方式');
      }
    } catch (err: any) {
      setActionError(err?.message || '移动失败');
    } finally {
      setActionLoading(false);
    }
  }, [canDropToDirectory, clearDragAutoScroll, clearDragExpandTimer, executeMoveEntry, findEntryByPath]);

  const handleMoveConflictCancel = useCallback(() => {
    setMoveConflict(null);
    setActionMessage('已取消移动');
  }, []);

  const handleMoveConflictOverwrite = useCallback(async () => {
    if (!moveConflict) return;
    setActionLoading(true);
    setActionError(null);
    try {
      await executeMoveEntry(
        moveConflict.sourcePath,
        moveConflict.targetDirPath,
        moveConflict.sourceName,
        { replaceExisting: true }
      );
      setActionMessage(`已覆盖并移动：${moveConflict.sourceName}`);
      setMoveConflict(null);
    } catch (err: any) {
      setActionError(err?.message || '覆盖移动失败');
    } finally {
      setActionLoading(false);
    }
  }, [executeMoveEntry, moveConflict]);

  const handleMoveConflictRename = useCallback(async () => {
    if (!moveConflict) return;
    const renamed = moveConflict.renameTo.trim();
    if (!renamed || !isValidEntryName(renamed)) {
      setActionError('新名称不合法');
      return;
    }
    setActionLoading(true);
    setActionError(null);
    try {
      await executeMoveEntry(
        moveConflict.sourcePath,
        moveConflict.targetDirPath,
        renamed,
        { targetName: renamed }
      );
      setMoveConflict(null);
    } catch (err: any) {
      setActionError(err?.message || '重命名移动失败');
    } finally {
      setActionLoading(false);
    }
  }, [executeMoveEntry, moveConflict]);

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

  const isEntryVisible = useCallback((entryPath: string): boolean => {
    if (!showOnlyChanged) return true;
    return aggregatedChangeKindByPath.has(normalizePath(entryPath));
  }, [aggregatedChangeKindByPath, normalizePath, showOnlyChanged]);

  const visibleRootEntryCount = useMemo(() => {
    if (!project?.rootPath) return 0;
    const rootEntries = entriesMap[project.rootPath] || [];
    return rootEntries.filter((entry) => isEntryVisible(entry.path)).length;
  }, [entriesMap, isEntryVisible, project?.rootPath]);

  const renderEntries = (path: string, depth: number): React.ReactNode => {
    const entries = (entriesMap[path] || []).filter((entry) => isEntryVisible(entry.path));
    if (!entries.length) {
      return null;
    }
    return entries.map((entry) => {
      const entryKey = toExpandedKey(entry.path);
      const normalizedEntryPath = normalizePath(entry.path);
      const isActive = selectedPath ? normalizePath(selectedPath) === normalizedEntryPath : false;
      const isDragging = draggingEntryPath ? normalizePath(draggingEntryPath) === normalizedEntryPath : false;
      const isDropTarget = entry.isDir && dropTargetDirPath
        ? normalizePath(dropTargetDirPath) === normalizedEntryPath
        : false;
      const entryChangeKind = aggregatedChangeKindByPath.get(normalizedEntryPath);
      return (
        <div key={entry.path}>
          <button
            type="button"
            onClick={() => (entry.isDir ? toggleDir(entry) : openFile(entry))}
            onContextMenu={(event) => openEntryContextMenu(event, entry)}
            draggable
            onDragStart={(event) => handleDragStart(event, entry)}
            onDragEnd={handleDragEnd}
            onDragOver={(event) => {
              if (!entry.isDir) return;
              const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
              if (!sourcePath || !canDropToDirectory(sourcePath, entry.path)) return;
              event.preventDefault();
              event.dataTransfer.dropEffect = 'move';
            }}
            onDragEnter={(event) => {
              if (!entry.isDir) return;
              const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
              if (!sourcePath || !canDropToDirectory(sourcePath, entry.path)) return;
              event.preventDefault();
              setDropTargetDirPath(entry.path);
              scheduleDragExpand(entry.path);
            }}
            onDragLeave={(event) => {
              if (!entry.isDir) return;
              const nextTarget = event.relatedTarget as Node | null;
              if (nextTarget && (event.currentTarget as HTMLElement).contains(nextTarget)) {
                return;
              }
              cancelDragExpandIfMatches(entry.path);
              clearDragAutoScroll();
              setDropTargetDirPath(prev => (
                prev && normalizePath(prev) === normalizePath(entry.path) ? null : prev
              ));
            }}
            onDrop={(event) => {
              if (!entry.isDir) return;
              const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
              if (!sourcePath) return;
              if (!canDropToDirectory(sourcePath, entry.path)) return;
              event.preventDefault();
              event.stopPropagation();
              cancelDragExpandIfMatches(entry.path);
              clearDragAutoScroll();
              setDropTargetDirPath(null);
              setDraggingEntryPath(null);
              void handleMoveEntryByDrop(sourcePath, entry.path);
            }}
            className={cn(
              'min-w-full w-max grid grid-cols-[12px_auto_64px] items-center gap-2 py-1.5 pr-2 text-left rounded hover:bg-accent transition-colors',
              entryChangeKind && CHANGE_KIND_ROW_CLASS[entryChangeKind],
              isActive && 'bg-accent',
              isDragging && 'opacity-50',
              isDropTarget && 'ring-1 ring-blue-500 bg-blue-500/10'
            )}
            style={{ paddingLeft: 12 + depth * 14 }}
          >
            <span className="text-xs text-muted-foreground w-3 shrink-0">
              {entry.isDir ? (expandedPaths.has(entryKey) ? '▾' : '▸') : ''}
            </span>
            <span
              className={cn(
                'text-sm whitespace-nowrap inline-flex items-center gap-1',
                entry.isDir ? 'text-foreground' : 'text-muted-foreground',
                entryChangeKind && CHANGE_KIND_TEXT_CLASS[entryChangeKind]
              )}
            >
              {entry.name}
              {entryChangeKind && (
                <span
                  className={cn('inline-block h-2 w-2 rounded-full', CHANGE_KIND_COLOR_CLASS[entryChangeKind])}
                  title={`未确认${CHANGE_KIND_LABEL[entryChangeKind]}变更`}
                />
              )}
            </span>
            <span className="text-[11px] text-muted-foreground text-right tabular-nums whitespace-nowrap">
              {!entry.isDir && entry.size != null ? formatFileSize(entry.size) : ''}
            </span>
          </button>
          {entry.isDir && expandedPaths.has(entryKey) && renderEntries(entry.path, depth + 1)}
        </div>
      );
    });
  };

  const selectedLog = useMemo(
    () => (selectedLogId ? changeLogs.find(log => log.id === selectedLogId) || null : null),
    [changeLogs, selectedLogId]
  );

  const preview = useMemo(() => {
    if (loadingFile) {
      return <div className="p-4 text-sm text-muted-foreground">加载文件中...</div>;
    }
    if (!selectedFile) {
      if (selectedPath && !selectedEntry) {
        return (
          <div className="p-4 text-sm text-muted-foreground">
            该路径已删除或不存在，当前仅支持查看变更记录。
          </div>
        );
      }
      return <div className="p-4 text-sm text-muted-foreground">请选择文件以预览</div>;
    }
    const isImage = selectedFile.contentType.startsWith('image/');
    if (isImage && selectedFile.isBinary) {
      const src = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
      return (
        <div className="p-4 overflow-auto h-full">
          <img src={src} alt={selectedFile.name} className="max-w-full max-h-full rounded border border-border" />
        </div>
      );
    }
    if (!selectedFile.isBinary) {
      const language = getHighlightLanguage(selectedFile.name);
      let highlighted = '';
      try {
        if (language) {
          highlighted = hljs.highlight(selectedFile.content, { language }).value;
        } else {
          highlighted = hljs.highlightAuto(selectedFile.content).value;
        }
      } catch {
        highlighted = escapeHtml(selectedFile.content);
      }
      const lines = highlighted.split(/\r?\n/);
      return (
        <div className="h-full overflow-auto bg-muted/30">
          <div className="flex min-h-full text-sm">
            <div className="shrink-0 py-4 pr-3 pl-2 border-r border-border text-right text-muted-foreground select-none">
              {lines.map((_, idx) => (
                <div key={idx} className="leading-5">
                  {idx + 1}
                </div>
              ))}
            </div>
            <div className="flex-1 min-w-0 py-4 pl-3 pr-4 hljs">
              {lines.map((line, idx) => (
                <div
                  key={idx}
                  className="leading-5 font-mono whitespace-pre w-full"
                  dangerouslySetInnerHTML={{ __html: line || '&nbsp;' }}
                />
              ))}
            </div>
          </div>
        </div>
      );
    }
    const downloadHref = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
    return (
      <div className="p-4 text-sm text-muted-foreground space-y-2">
        <div>该文件为二进制内容，暂不支持直接预览。</div>
        <a
          href={downloadHref}
          download={selectedFile.name || 'file'}
          className="inline-flex items-center px-3 py-1.5 rounded bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
        >
          下载文件
        </a>
      </div>
    );
  }, [selectedEntry, selectedFile, selectedPath, loadingFile]);

  const parseUnifiedDiff = useCallback((diffText: string) => {
    const lines = diffText.split(/\r?\n/);
    const parsed: Array<{ type: 'meta' | 'hunk' | 'add' | 'del' | 'context'; oldLine?: number | null; newLine?: number | null; text: string }> = [];
    let oldLine = 0;
    let newLine = 0;
    let inHunk = false;
    const hunkRegex = /^@@\s+-(\d+)(?:,(\d+))?\s+\+(\d+)(?:,(\d+))?\s+@@/;
    for (const line of lines) {
      const hunkMatch = hunkRegex.exec(line);
      if (hunkMatch) {
        oldLine = parseInt(hunkMatch[1], 10);
        newLine = parseInt(hunkMatch[3], 10);
        inHunk = true;
        parsed.push({ type: 'hunk', text: line });
        continue;
      }
      if (!inHunk) {
        parsed.push({ type: 'meta', text: line });
        continue;
      }
      if (line.startsWith('+++') || line.startsWith('---')) {
        parsed.push({ type: 'meta', text: line });
        continue;
      }
      if (line.startsWith('+')) {
        parsed.push({ type: 'add', oldLine: null, newLine, text: line });
        newLine += 1;
        continue;
      }
      if (line.startsWith('-')) {
        parsed.push({ type: 'del', oldLine, newLine: null, text: line });
        oldLine += 1;
        continue;
      }
      if (line.startsWith('\\')) {
        parsed.push({ type: 'meta', text: line });
        continue;
      }
      parsed.push({ type: 'context', oldLine, newLine, text: line });
      oldLine += 1;
      newLine += 1;
    }
    return parsed;
  }, []);

  const renderDiffRows = useCallback((diffText: string) => {
    const rows = parseUnifiedDiff(diffText);
    if (!rows.length) {
      return <div className="text-muted-foreground">该记录没有 diff 内容</div>;
    }
    return (
      <div className="font-mono text-xs">
        {rows.map((row, idx) => {
          let lineClass = 'text-foreground';
          if (row.type === 'hunk' || row.type === 'meta') {
            lineClass = 'text-muted-foreground';
          } else if (row.type === 'add') {
            lineClass = 'text-emerald-600 dark:text-emerald-400';
          } else if (row.type === 'del') {
            lineClass = 'text-rose-600 dark:text-rose-400';
          }
          return (
            <div key={`${idx}-${row.text}`} className={cn('grid grid-cols-[3rem_3rem_1fr] gap-2 leading-5', lineClass)}>
              <div className="text-right pr-2 text-muted-foreground">
                {row.oldLine ?? ''}
              </div>
              <div className="text-right pr-2 text-muted-foreground">
                {row.newLine ?? ''}
              </div>
              <div className="whitespace-pre">
                {row.text === '' ? ' ' : row.text}
              </div>
            </div>
          );
        })}
      </div>
    );
  }, [parseUnifiedDiff]);

  const diffPanel = useMemo(() => {
    if (!selectedLog) return null;
    const title = selectedLog.sessionTitle || selectedLog.sessionId || '未知会话';
    const time = selectedLog.createdAt ? new Date(selectedLog.createdAt).toLocaleString() : '';
    const kind = normalizeChangeKind(selectedLog.changeKind);
    return (
      <div className="border-b border-border bg-muted/30 max-h-64 overflow-hidden flex flex-col">
        <div className="px-4 py-2 text-xs font-medium text-foreground flex items-center gap-2">
          <span>变更内容</span>
          <span className="text-muted-foreground">{selectedLog.action}</span>
          <span className={CHANGE_KIND_TEXT_CLASS[kind]}>{CHANGE_KIND_LABEL[kind]}</span>
          <span className="text-muted-foreground ml-auto">{time}</span>
        </div>
        <div className="px-4 pb-3 text-xs overflow-auto min-h-0">
          <div className="text-[11px] text-muted-foreground mb-2 truncate" title={title}>
            会话：{title}
          </div>
          {selectedLog.diff ? renderDiffRows(selectedLog.diff) : (
            <div className="text-muted-foreground">该记录没有 diff 内容</div>
          )}
        </div>
      </div>
    );
  }, [selectedLog, renderDiffRows]);

  const changeLogPanel = useMemo(() => {
    if (!selectedPath) {
      return <div className="px-4 py-3 text-xs text-muted-foreground">请选择文件或目录以查看变更记录</div>;
    }
    if (loadingLogs) {
      return <div className="px-4 py-3 text-xs text-muted-foreground">加载变更记录中...</div>;
    }
    if (logsError) {
      return <div className="px-4 py-3 text-xs text-destructive">{logsError}</div>;
    }
    if (!changeLogs.length) {
      return <div className="px-4 py-3 text-xs text-muted-foreground">暂无变更记录</div>;
    }
    return (
      <div className="divide-y divide-border">
        {changeLogs.map((log) => {
          const isSelected = selectedLogId === log.id;
          const title = log.sessionTitle || log.sessionId || '未知会话';
          const time = log.createdAt ? new Date(log.createdAt).toLocaleString() : '';
          const kind = normalizeChangeKind(log.changeKind);
          return (
            <button
              key={log.id}
              type="button"
              onClick={() => setSelectedLogId(prev => (prev === log.id ? null : log.id))}
              className={cn(
                'w-full px-4 py-2 text-xs text-left hover:bg-accent transition-colors',
                isSelected && 'bg-accent'
              )}
            >
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground w-3">{isSelected ? '▾' : '▸'}</span>
                <span className="font-medium text-foreground">{log.action}</span>
                <span className={cn('font-medium', CHANGE_KIND_TEXT_CLASS[kind])}>
                  {CHANGE_KIND_LABEL[kind]}
                </span>
                <span className="text-muted-foreground">{formatFileSize(log.bytes || 0)}</span>
                <span className="text-muted-foreground ml-auto">{time}</span>
              </div>
              <div className="text-[11px] text-muted-foreground truncate" title={title}>
                会话：{title}
              </div>
            </button>
          );
        })}
      </div>
    );
  }, [selectedPath, loadingLogs, logsError, changeLogs, selectedLogId]);

  if (!project) {
    return (
      <div className={cn('flex items-center justify-center h-full text-muted-foreground', className)}>
        请选择一个项目查看文件
      </div>
    );
  }

  return (
    <div ref={containerRef} className={cn('flex h-full overflow-hidden', className)}>
      <div className="border-r border-border bg-card flex flex-col shrink-0" style={{ width: treeWidth }}>
        <div
          className={cn(
            'px-3 py-2 border-b border-border space-y-2',
            dropTargetDirPath && project.rootPath && normalizePath(dropTargetDirPath) === normalizePath(project.rootPath)
              ? 'ring-1 ring-blue-500 bg-blue-500/10'
              : ''
          )}
          onContextMenu={(event) => {
            if (projectRootEntry) {
              openEntryContextMenu(event, projectRootEntry);
            }
          }}
          onDragOver={(event) => {
            const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
            if (!sourcePath || !project.rootPath) return;
            if (!canDropToDirectory(sourcePath, project.rootPath)) return;
            event.preventDefault();
            event.dataTransfer.dropEffect = 'move';
          }}
          onDragEnter={(event) => {
            const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
            if (!sourcePath || !project.rootPath) return;
            if (!canDropToDirectory(sourcePath, project.rootPath)) return;
            event.preventDefault();
            clearDragExpandTimer();
            clearDragAutoScroll();
            setDropTargetDirPath(project.rootPath);
          }}
          onDragLeave={(event) => {
            const nextTarget = event.relatedTarget as Node | null;
            if (nextTarget && (event.currentTarget as HTMLElement).contains(nextTarget)) {
              return;
            }
            if (project.rootPath) {
              const normalizedRoot = normalizePath(project.rootPath);
              setDropTargetDirPath(prev => (
                prev && normalizePath(prev) === normalizedRoot ? null : prev
              ));
            }
          }}
          onDrop={(event) => {
            const sourcePath = draggingEntryPath || event.dataTransfer.getData('text/plain');
            if (!sourcePath || !project.rootPath) return;
            if (!canDropToDirectory(sourcePath, project.rootPath)) return;
            event.preventDefault();
            event.stopPropagation();
            clearDragExpandTimer();
            clearDragAutoScroll();
            setDropTargetDirPath(null);
            setDraggingEntryPath(null);
            void handleMoveEntryByDrop(sourcePath, project.rootPath);
          }}
        >
          <div className="text-xs text-muted-foreground">项目目录</div>
          <div className="text-sm font-medium text-foreground truncate" title={project.rootPath}>
            {project.name}
          </div>
          <div className="text-[11px] text-muted-foreground truncate" title={project.rootPath}>
            {project.rootPath}
          </div>
          <div className="text-[11px] text-muted-foreground truncate" title={selectedEntry?.path || ''}>
            当前选择：{selectedEntry ? selectedEntry.path : '未选择'}
          </div>
          <div className="text-[11px] text-muted-foreground flex items-center gap-3">
            <span className="inline-flex items-center gap-1">
              <span className="inline-block h-2 w-2 rounded-full bg-emerald-500" />
              新增 {changeSummary.counts.create}
            </span>
            <span className="inline-flex items-center gap-1">
              <span className="inline-block h-2 w-2 rounded-full bg-amber-500" />
              编辑 {changeSummary.counts.edit}
            </span>
            <span className="inline-flex items-center gap-1">
              <span className="inline-block h-2 w-2 rounded-full bg-rose-500" />
              删除 {changeSummary.counts.delete}
            </span>
          </div>
          <div className="flex flex-wrap gap-1">
            <button
              type="button"
              onClick={() => {
                void handleCreateDirectory();
              }}
              disabled={!selectedDirPath || actionLoading}
              className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
            >
              新建目录
            </button>
            <button
              type="button"
              onClick={() => {
                void handleCreateFile();
              }}
              disabled={!selectedDirPath || actionLoading}
              className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
            >
              新建文件
            </button>
            <button
              type="button"
              onClick={() => {
                void handleDeleteSelected();
              }}
              disabled={!selectedEntry || isRootSelected || actionLoading}
              className="rounded border border-border px-2 py-1 text-[11px] text-destructive hover:bg-destructive/10 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              删除
            </button>
            <button
              type="button"
              onClick={() => {
                void handleDownloadSelected();
              }}
              disabled={!selectedEntry || actionLoading}
              className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
            >
              下载
            </button>
            <button
              type="button"
              onClick={() => {
                void handleRefresh();
              }}
              disabled={!actionReloadPath || actionLoading}
              className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
            >
              刷新
            </button>
            <button
              type="button"
              onClick={() => {
                void handleConfirmCurrentChanges();
              }}
              disabled={!canConfirmCurrent || actionLoading}
              className="rounded border border-amber-500/40 px-2 py-1 text-[11px] text-amber-700 hover:bg-amber-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              确认当前项
            </button>
            <button
              type="button"
              onClick={() => {
                void handleConfirmAllChanges();
              }}
              disabled={changeSummary.counts.total <= 0 || actionLoading}
              className="rounded border border-emerald-500/40 px-2 py-1 text-[11px] text-emerald-700 hover:bg-emerald-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              确认全部变更
            </button>
            <button
              type="button"
              onClick={() => {
                setShowOnlyChanged((prev) => !prev);
              }}
              className={cn(
                'rounded border px-2 py-1 text-[11px] disabled:opacity-50 disabled:cursor-not-allowed',
                showOnlyChanged
                  ? 'border-emerald-500/50 text-emerald-700 bg-emerald-500/10 hover:bg-emerald-500/20'
                  : 'border-border hover:bg-accent'
              )}
            >
              {showOnlyChanged ? '显示全部' : '仅看变更'}
            </button>
          </div>
          {loadingSummary && (
            <div className="text-[11px] text-muted-foreground">正在加载变更标记...</div>
          )}
          {summaryError && (
            <div className="text-[11px] text-destructive truncate" title={summaryError}>
              {summaryError}
            </div>
          )}
          {actionMessage && (
            <div className="text-[11px] text-emerald-600 truncate" title={actionMessage}>
              {actionMessage}
            </div>
          )}
          {actionError && (
            <div className="text-[11px] text-destructive truncate" title={actionError}>
              {actionError}
            </div>
          )}
        </div>
        <div
          ref={treeScrollRef}
          className="flex-1 overflow-y-auto overflow-x-auto py-2"
          onDragOver={(event) => {
            if (!draggingEntryPath) return;
            const container = treeScrollRef.current;
            if (!container) return;
            const rect = container.getBoundingClientRect();
            const threshold = Math.max(28, Math.min(64, rect.height / 3));
            let velocity = 0;

            if (event.clientY < rect.top + threshold) {
              const ratio = (rect.top + threshold - event.clientY) / threshold;
              velocity = -Math.max(4, Math.round(22 * ratio));
            } else if (event.clientY > rect.bottom - threshold) {
              const ratio = (event.clientY - (rect.bottom - threshold)) / threshold;
              velocity = Math.max(4, Math.round(22 * ratio));
            }

            if (velocity !== 0) {
              event.preventDefault();
              startDragAutoScroll(velocity);
            } else {
              clearDragAutoScroll();
            }
          }}
          onDragLeave={(event) => {
            const nextTarget = event.relatedTarget as Node | null;
            if (nextTarget && (event.currentTarget as HTMLElement).contains(nextTarget)) {
              return;
            }
            clearDragAutoScroll();
          }}
          onDrop={() => {
            clearDragAutoScroll();
          }}
        >
          {renderEntries(project.rootPath, 0)}
          {changeSummary.deletedMarks.length > 0 && (
            <div className="mt-2 border-t border-border/70">
              <div className="px-3 py-2 text-[11px] font-medium text-rose-600 dark:text-rose-400">
                已删除（未确认）
              </div>
              <div className="space-y-0.5 pb-2">
                {changeSummary.deletedMarks.map((mark) => {
                  const normalizedMarkPath = normalizePath(mark.path);
                  const isActive = selectedPath ? normalizePath(selectedPath) === normalizedMarkPath : false;
                  return (
                    <button
                      key={mark.lastChangeId || mark.path}
                      type="button"
                      onClick={() => {
                        setSelectedPath(mark.path);
                        setSelectedFile(null);
                      }}
                      className={cn(
                        'min-w-full w-max grid grid-cols-[12px_auto_64px] items-center gap-2 py-1.5 pr-2 text-left rounded hover:bg-accent transition-colors',
                        isActive && 'bg-accent'
                      )}
                      style={{ paddingLeft: 12 + 14 }}
                    >
                      <span className="text-xs text-rose-500 w-3 shrink-0">•</span>
                      <span className={cn('text-sm whitespace-nowrap truncate', CHANGE_KIND_TEXT_CLASS.delete)}>
                        {mark.relativePath || mark.path}
                      </span>
                      <span className="text-[11px] text-muted-foreground text-right tabular-nums whitespace-nowrap">
                        已删除
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          )}
          {loadingPaths.has(project.rootPath) && (
            <div className="px-3 py-2 text-xs text-muted-foreground">加载中...</div>
          )}
          {!loadingPaths.has(project.rootPath) && visibleRootEntryCount === 0 && (
            <div className="px-3 py-2 text-xs text-muted-foreground">
              {showOnlyChanged ? '暂无未确认变更文件' : '目录为空'}
            </div>
          )}
        </div>
      </div>
      <div
        className={cn('w-1 cursor-col-resize bg-border/60 hover:bg-border', isResizing && 'bg-border')}
        onMouseDown={(event) => {
          resizeStartX.current = event.clientX;
          resizeStartWidth.current = treeWidth;
          setIsResizing(true);
        }}
      />
      <div className="flex-1 flex overflow-hidden">
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="px-4 py-2 border-b border-border bg-card flex items-center justify-between">
            <div className="min-w-0">
              <div className="text-sm font-medium text-foreground truncate">
                {selectedFile?.name || (selectedPath ? '文件预览（当前项不可预览）' : '文件预览')}
              </div>
              <div className="text-[11px] text-muted-foreground truncate">
                {selectedFile?.path || selectedPath || '请选择文件'}
              </div>
            </div>
            {selectedFile && (
              <div className="text-[11px] text-muted-foreground">
                {formatFileSize(selectedFile.size)}
              </div>
            )}
          </div>
          <div className="flex-1 overflow-hidden flex flex-col">
            {diffPanel}
            <div className="flex-1 min-h-0 overflow-hidden">
              {error ? (
                <div className="p-4 text-sm text-destructive">{error}</div>
              ) : (
                preview
              )}
            </div>
          </div>
        </div>
        {(loadingLogs || logsError || changeLogs.length > 0) && (
          <div className="w-72 border-l border-border bg-card/60 flex flex-col overflow-hidden">
            <div className="px-4 py-2 text-xs font-medium text-foreground border-b border-border">变更记录</div>
            <div className="flex-1 min-h-0 overflow-auto">
              {changeLogPanel}
            </div>
          </div>
        )}
      </div>
      {moveConflict && (
        <div
          className="fixed inset-0 z-[90] bg-black/35 flex items-center justify-center p-4"
          onClick={() => {
            if (!actionLoading) {
              handleMoveConflictCancel();
            }
          }}
        >
          <div
            className="w-full max-w-md rounded-lg border border-border bg-card p-4 shadow-xl"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="text-sm font-medium text-foreground">目标目录存在同名项</div>
            <div className="mt-2 text-xs text-muted-foreground">
              将 {moveConflict.sourceName} 移动到目标目录时发生冲突，请选择处理方式。
            </div>
            <div className="mt-3 space-y-1.5">
              <label className="text-xs text-muted-foreground">重命名后移动</label>
              <input
                value={moveConflict.renameTo}
                onChange={(event) => {
                  const value = event.target.value;
                  setMoveConflict((prev) => (prev ? { ...prev, renameTo: value } : prev));
                }}
                className="w-full h-9 rounded border border-input bg-background px-2 text-sm"
                placeholder="请输入新名称"
              />
            </div>
            <div className="mt-4 flex justify-end gap-2">
              <button
                type="button"
                onClick={handleMoveConflictCancel}
                disabled={actionLoading}
                className="px-3 py-1.5 text-xs rounded border border-border hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => {
                  void handleMoveConflictOverwrite();
                }}
                disabled={actionLoading}
                className="px-3 py-1.5 text-xs rounded border border-amber-500/50 text-amber-700 hover:bg-amber-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                覆盖后移动
              </button>
              <button
                type="button"
                onClick={() => {
                  void handleMoveConflictRename();
                }}
                disabled={actionLoading}
                className="px-3 py-1.5 text-xs rounded bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                重命名后移动
              </button>
            </div>
          </div>
        </div>
      )}
      {contextMenu && contextMenuStyle && (
        <div
          className="fixed z-[80] w-56 rounded-md border border-border bg-popover text-popover-foreground shadow-lg p-1"
          style={contextMenuStyle}
          onClick={(event) => event.stopPropagation()}
          onContextMenu={(event) => event.preventDefault()}
        >
          <div className="px-2 py-1 text-[11px] text-muted-foreground truncate">
            {contextMenu.entry.isDir ? '目录' : '文件'}：{contextMenu.entry.path}
          </div>
          {contextMenu.entry.isDir && (
            <button
              type="button"
              onClick={() => {
                const targetPath = contextMenu.entry.path;
                setContextMenu(null);
                void handleCreateDirectory(targetPath);
              }}
              className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
            >
              新建目录
            </button>
          )}
          {contextMenu.entry.isDir && (
            <button
              type="button"
              onClick={() => {
                const targetPath = contextMenu.entry.path;
                setContextMenu(null);
                void handleCreateFile(targetPath);
              }}
              className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
            >
              新建文件
            </button>
          )}
          <button
            type="button"
            onClick={() => {
              const targetEntry = contextMenu.entry;
              setContextMenu(null);
              void handleDownloadSelected(targetEntry);
            }}
            className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
          >
            下载
          </button>
          <button
            type="button"
            onClick={() => {
              const targetEntry = contextMenu.entry;
              setContextMenu(null);
              void handleDeleteSelected(targetEntry);
            }}
            disabled={isContextRootEntry}
            className="w-full text-left px-2 py-1.5 text-sm rounded text-destructive hover:bg-destructive/10 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            删除
          </button>
        </div>
      )}
    </div>
  );
};

export default ProjectExplorer;
