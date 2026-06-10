import { useCallback, useMemo } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import type ApiClient from '../../lib/api/client';
import type {
  CodeNavLocation,
  FsEntry,
  FsReadResult,
  Project,
} from '../../types';
import { normalizeFile } from './utils';

interface UseProjectExplorerSelectionParams {
  client: ApiClient;
  project: Project | null;
  entriesMap: Record<string, FsEntry[]>;
  selectedPath: string | null;
  clearSearchNavigation: () => void;
  normalizePath: (path: string) => string;
  getParentPath: (path: string | null | undefined) => string;
  loadEntries: (path: string, options?: { silent?: boolean; forceRefresh?: boolean }) => Promise<void>;
  setActionError: (value: string | null) => void;
  setSelectedPath: (value: string | null) => void;
  setSelectedFile: (value: FsReadResult | null) => void;
  setLoadingFile: (value: boolean) => void;
  setError: (value: string | null) => void;
  setPreviewTargetLine: (line: number | null) => void;
}

const readErrorMessage = (error: unknown, fallback: string): string => (
  error instanceof Error ? error.message : fallback
);

export const useProjectExplorerSelection = ({
  client,
  project,
  entriesMap,
  selectedPath,
  clearSearchNavigation,
  normalizePath,
  getParentPath,
  loadEntries,
  setActionError,
  setSelectedPath,
  setSelectedFile,
  setLoadingFile,
  setError,
  setPreviewTargetLine,
}: UseProjectExplorerSelectionParams) => {
  const { t } = useI18n();

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
    [selectedEntry],
  );

  const actionReloadPath = useMemo(() => {
    if (!selectedEntry) return project?.rootPath || null;
    if (selectedEntry.isDir) return selectedEntry.path;
    return getParentPath(selectedEntry.path) || project?.rootPath || null;
  }, [getParentPath, project?.rootPath, selectedEntry]);

  const openFile = useCallback(async (entry: FsEntry) => {
    clearSearchNavigation();
    setActionError(null);
    setSelectedPath(entry.path);
    setSelectedFile(null);
    setLoadingFile(true);
    setError(null);
    try {
      const data = await client.readFsFile(entry.path);
      setSelectedFile(normalizeFile(data));
    } catch (error) {
      setError(readErrorMessage(error, t('projectExplorer.error.readFile')));
    } finally {
      setLoadingFile(false);
    }
  }, [
    clearSearchNavigation,
    client,
    setActionError,
    setError,
    setLoadingFile,
    setSelectedFile,
    setSelectedPath,
    t,
  ]);

  const openCodeNavLocation = useCallback(async (
    location: CodeNavLocation,
    options?: {
      preserveHistory?: boolean;
      targetLine?: number | null;
    },
  ) => {
    await openFile({
      name: location.relativePath.split('/').filter(Boolean).pop() || location.path.split(/[\\/]/).pop() || location.path,
      path: location.path,
      isDir: false,
      size: null,
      modifiedAt: null,
    });
    setPreviewTargetLine(options?.targetLine ?? location.line);
  }, [openFile, setPreviewTargetLine]);

  const selectProjectRoot = useCallback(async () => {
    const root = project?.rootPath;
    if (!root) return;
    setSelectedPath(root);
    setSelectedFile(null);
    if (!entriesMap[root]) {
      await loadEntries(root);
    }
  }, [entriesMap, loadEntries, project?.rootPath, setSelectedFile, setSelectedPath]);

  return {
    actionReloadPath,
    findEntryByPath,
    openCodeNavLocation,
    openFile,
    projectRootEntry,
    selectProjectRoot,
    selectedDirPath,
    selectedEntry,
  };
};
