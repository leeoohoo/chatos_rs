import { useCallback } from 'react';

import {
  readProjectTreeConfirmedCount,
  readProjectTreeErrorMessage,
} from './projectTreeActionHelpers';
import type { UseProjectTreeActionsOptions } from './projectTreeActionTypes';

type UseProjectTreeChangeActionsOptions = Pick<
  UseProjectTreeActionsOptions,
  | 'client'
  | 'projectId'
  | 'selectedPath'
  | 'hasPendingChangesForPath'
  | 'loadChangeSummary'
  | 'setActionLoading'
  | 'setActionError'
  | 'setActionMessage'
>;

export const useProjectTreeChangeActions = ({
  client,
  projectId,
  selectedPath,
  hasPendingChangesForPath,
  loadChangeSummary,
  setActionLoading,
  setActionError,
  setActionMessage,
}: UseProjectTreeChangeActionsOptions) => {
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
      const confirmed = readProjectTreeConfirmedCount(result);
      if (Number.isFinite(confirmed) && confirmed > 0) {
        setActionMessage(`已确认当前项变更（${confirmed} 条）`);
      } else {
        setActionMessage('当前项没有可确认的变更');
      }
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, '确认当前项变更失败'));
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
      const confirmed = readProjectTreeConfirmedCount(result);
      if (Number.isFinite(confirmed) && confirmed > 0) {
        setActionMessage(`已确认全部变更（${confirmed} 条）`);
      } else {
        setActionMessage('暂无可确认的变更');
      }
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, '确认全部变更失败'));
    } finally {
      setActionLoading(false);
    }
  }, [client, loadChangeSummary, projectId, setActionError, setActionLoading, setActionMessage]);

  return {
    handleConfirmCurrentChanges,
    handleConfirmAllChanges,
  };
};
