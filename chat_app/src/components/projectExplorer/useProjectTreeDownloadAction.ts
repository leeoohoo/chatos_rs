import { useCallback } from 'react';

import type { FsEntry } from '../../types';
import { readProjectTreeErrorMessage } from './projectTreeActionHelpers';
import type { UseProjectTreeActionsOptions } from './projectTreeActionTypes';

type UseProjectTreeDownloadActionOptions = Pick<
  UseProjectTreeActionsOptions,
  | 'client'
  | 'selectedEntry'
  | 'setActionLoading'
  | 'setActionError'
  | 'setActionMessage'
>;

export const useProjectTreeDownloadAction = ({
  client,
  selectedEntry,
  setActionLoading,
  setActionError,
  setActionMessage,
}: UseProjectTreeDownloadActionOptions) => {
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
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, '下载失败'));
    } finally {
      setActionLoading(false);
    }
  }, [client, selectedEntry, setActionError, setActionLoading, setActionMessage]);

  return {
    handleDownloadSelected,
  };
};
