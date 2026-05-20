import { useCallback } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { copyToClipboard } from '../../lib/utils';
import type { FsEntry } from '../../types';
import { readProjectTreeErrorMessage } from './projectTreeActionHelpers';
import type { UseProjectTreeActionsOptions } from './projectTreeActionTypes';

type UseProjectTreeContextActionsOptions = Pick<
  UseProjectTreeActionsOptions,
  | 'client'
  | 'projectRootPath'
  | 'normalizePath'
  | 'loadEntries'
  | 'setActionLoading'
  | 'setActionError'
  | 'setActionMessage'
>;

const toRelativeProjectPath = (
  projectRootPath: string | null | undefined,
  targetPath: string,
  normalizePath: (value: string) => string,
): string => {
  const normalizedRoot = normalizePath(projectRootPath || '');
  const normalizedTarget = normalizePath(targetPath);
  if (!normalizedRoot || !normalizedTarget) {
    return targetPath;
  }
  if (normalizedTarget === normalizedRoot) {
    return '.';
  }
  const prefix = `${normalizedRoot}/`;
  if (normalizedTarget.startsWith(prefix)) {
    return normalizedTarget.slice(prefix.length) || '.';
  }
  return targetPath;
};

export const useProjectTreeContextActions = ({
  client,
  projectRootPath,
  normalizePath,
  loadEntries,
  setActionLoading,
  setActionError,
  setActionMessage,
}: UseProjectTreeContextActionsOptions) => {
  const { t } = useI18n();

  const refreshAfterMutation = useCallback(async (entryPath: string) => {
    const targetPath = projectRootPath || entryPath;
    await loadEntries(targetPath, { silent: true, forceRefresh: true });
  }, [loadEntries, projectRootPath]);

  const handleCopyFilePath = useCallback(async (entry: FsEntry) => {
    const success = await copyToClipboard(entry.path);
    if (!success) {
      setActionError(t('projectExplorer.context.copyPathFailed'));
      return false;
    }
    setActionMessage(t('projectExplorer.context.copyPathSuccess'));
    return true;
  }, [setActionError, setActionMessage, t]);

  const handleCopyRelativeFilePath = useCallback(async (entry: FsEntry) => {
    const relativePath = toRelativeProjectPath(projectRootPath, entry.path, normalizePath);
    const success = await copyToClipboard(relativePath);
    if (!success) {
      setActionError(t('projectExplorer.context.copyRelativePathFailed'));
      return false;
    }
    setActionMessage(t('projectExplorer.context.copyRelativePathSuccess'));
    return true;
  }, [normalizePath, projectRootPath, setActionError, setActionMessage, t]);

  const handleAppendGitignore = useCallback(async (
    entry: FsEntry,
    mode: 'file' | 'folder' | 'extension',
  ) => {
    setActionLoading(true);
    setActionError(null);
    try {
      const result = await client.appendFsGitignore(entry.path, mode);
      await refreshAfterMutation(entry.path);
      const pattern = typeof result.pattern === 'string' && result.pattern.trim()
        ? result.pattern.trim()
        : entry.name;
      if (result.appended === false) {
        setActionMessage(t('projectExplorer.context.gitignoreExists', { pattern }));
      } else {
        setActionMessage(t('projectExplorer.context.gitignoreSuccess', { pattern }));
      }
      return true;
    } catch (error) {
      setActionError(readProjectTreeErrorMessage(error, t('projectExplorer.context.gitignoreFailed')));
      return false;
    } finally {
      setActionLoading(false);
    }
  }, [client, refreshAfterMutation, setActionError, setActionLoading, setActionMessage, t]);

  const handleOpenExternally = useCallback(async (
    entry: FsEntry,
    mode: 'default' | 'reveal' | 'code',
  ) => {
    setActionLoading(true);
    setActionError(null);
    try {
      await client.openFsPathExternally(entry.path, mode);
      setActionMessage(
        mode === 'reveal'
          ? t('projectExplorer.context.revealSuccess')
          : mode === 'code'
            ? t('projectExplorer.context.openCodeSuccess')
            : t('projectExplorer.context.openDefaultSuccess'),
      );
      return true;
    } catch (error) {
      setActionError(readProjectTreeErrorMessage(error, t('projectExplorer.context.openFailed')));
      return false;
    } finally {
      setActionLoading(false);
    }
  }, [client, setActionError, setActionLoading, setActionMessage, t]);

  return {
    handleCopyFilePath,
    handleCopyRelativeFilePath,
    handleAppendGitignore,
    handleOpenExternally,
  };
};
