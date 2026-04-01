import { useCallback } from 'react';
import type { Dispatch, SetStateAction } from 'react';

import type { FsEntry, FsReadResult } from '../../types';
import { isValidEntryName } from './utils';
import type { MoveConflictState } from './Overlays';

interface UseProjectTreeActionsOptions {
  client: any;
  selectedDirPath: string | null;
  selectedEntry: FsEntry | null;
  selectedFilePath: string | null;
  selectedPath: string | null;
  projectRootPath?: string | null;
  projectId?: string | null;
  actionReloadPath: string | null;
  normalizePath: (value: string) => string;
  getParentPath: (value: string) => string | null;
  toExpandedKey: (path: string) => string;
  loadEntries: (path: string) => Promise<void>;
  loadChangeSummary: (options?: { silent?: boolean }) => Promise<void>;
  hasPendingChangesForPath: (path: string | null) => boolean;
  pruneDeletedPath: (deletedPath: string) => void;
  replaceExpandedPathPrefix: (sourcePath: string, movedPath: string) => Set<string>;
  reloadTreeWithExpanded: (nextExpanded: Set<string>) => Promise<void>;
  canDropToDirectory: (sourcePath: string, targetDirPath: string) => boolean;
  findEntryByPath: (path: string) => FsEntry | null;
  clearDragExpandTimer: () => void;
  clearDragAutoScroll: () => void;
  setExpandedPaths: Dispatch<SetStateAction<Set<string>>>;
  setSelectedPath: Dispatch<SetStateAction<string | null>>;
  setSelectedFile: Dispatch<SetStateAction<FsReadResult | null>>;
  setActionLoading: Dispatch<SetStateAction<boolean>>;
  setActionError: Dispatch<SetStateAction<string | null>>;
  setActionMessage: Dispatch<SetStateAction<string | null>>;
  setMoveConflict: Dispatch<SetStateAction<MoveConflictState | null>>;
  openFile: (entry: FsEntry) => Promise<void>;
}

export const useProjectTreeActions = ({
  client,
  selectedDirPath,
  selectedEntry,
  selectedFilePath,
  selectedPath,
  projectRootPath,
  projectId,
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
}: UseProjectTreeActionsOptions) => {
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
  }, [
    client,
    loadEntries,
    selectedDirPath,
    setActionError,
    setActionLoading,
    setActionMessage,
    setExpandedPaths,
    setMoveConflict,
    toExpandedKey,
  ]);

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
  }, [
    client,
    loadEntries,
    openFile,
    selectedDirPath,
    setActionError,
    setActionLoading,
    setActionMessage,
    setExpandedPaths,
    toExpandedKey,
  ]);

  const handleDeleteSelected = useCallback(async (entryOverride?: FsEntry) => {
    const targetEntry = entryOverride || selectedEntry;
    if (!targetEntry) {
      setActionError('请先选择要删除的文件或目录');
      return;
    }
    const targetIsRoot = !!projectRootPath
      && normalizePath(targetEntry.path) === normalizePath(projectRootPath);
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
      if (selectedFilePath && normalizePath(selectedFilePath) === normalizePath(targetEntry.path)) {
        setSelectedFile(null);
      }

      const fallbackPath = getParentPath(targetEntry.path) || projectRootPath || null;
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
    projectRootPath,
    pruneDeletedPath,
    selectedEntry,
    selectedFilePath,
    setActionError,
    setActionLoading,
    setActionMessage,
    setSelectedFile,
    setSelectedPath,
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
  }, [client, selectedEntry, setActionError, setActionLoading, setActionMessage]);

  const handleRefresh = useCallback(async () => {
    if (!actionReloadPath) return;
    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      await loadEntries(actionReloadPath);
      if (
        projectRootPath
        && normalizePath(actionReloadPath) !== normalizePath(projectRootPath)
      ) {
        await loadEntries(projectRootPath);
      }
      await loadChangeSummary();
      setActionMessage('目录已刷新');
    } catch (err: any) {
      setActionError(err?.message || '刷新失败');
    } finally {
      setActionLoading(false);
    }
  }, [
    actionReloadPath,
    loadChangeSummary,
    loadEntries,
    normalizePath,
    projectRootPath,
    setActionError,
    setActionLoading,
    setActionMessage,
  ]);

  const handleConfirmCurrentChanges = useCallback(async () => {
    if (!projectId) return;
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
      const result = await client.confirmProjectChanges(projectId, {
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
  }, [
    client,
    hasPendingChangesForPath,
    loadChangeSummary,
    projectId,
    selectedPath,
    setActionError,
    setActionLoading,
    setActionMessage,
  ]);

  const handleConfirmAllChanges = useCallback(async () => {
    if (!projectId) return;

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const result = await client.confirmProjectChanges(projectId, { mode: 'all' });
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
  }, [client, loadChangeSummary, projectId, setActionError, setActionLoading, setActionMessage]);

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
  }, [
    reloadTreeWithExpanded,
    replaceExpandedPathPrefix,
    setActionMessage,
    setExpandedPaths,
    setSelectedFile,
    setSelectedPath,
    toExpandedKey,
  ]);

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
  }, [
    canDropToDirectory,
    clearDragAutoScroll,
    clearDragExpandTimer,
    executeMoveEntry,
    findEntryByPath,
    setActionError,
    setActionLoading,
    setActionMessage,
    setMoveConflict,
  ]);

  const handleMoveConflictCancel = useCallback(() => {
    setMoveConflict(null);
    setActionMessage('已取消移动');
  }, [setActionMessage, setMoveConflict]);

  const handleMoveConflictOverwrite = useCallback(async (moveConflict: MoveConflictState | null) => {
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
  }, [executeMoveEntry, setActionError, setActionLoading, setActionMessage, setMoveConflict]);

  const handleMoveConflictRename = useCallback(async (moveConflict: MoveConflictState | null) => {
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
  }, [executeMoveEntry, setActionError, setActionLoading, setMoveConflict]);

  return {
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
  };
};
