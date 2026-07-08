// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type { GitActionResponse } from '../../../lib/api/client/types';
import type { GitBranchInfo } from '../../../types';
import type { ProjectGitApiClient } from './projectGitTypes';
import { normalizeNonEmptyPaths } from './projectGitHelpers';

interface UseProjectGitActionsParams {
  client: ProjectGitApiClient;
  projectRoot: string | null;
  summary: {
    dirty?: boolean;
    detached?: boolean;
    currentBranch?: string | null;
    operationState?: string | null;
  } | null;
  confirm: (options: {
    title: string;
    message: string;
    confirmText?: string;
    cancelText?: string;
    type?: 'warning' | 'danger' | 'info';
  }) => Promise<boolean>;
  runAction: (
    action: () => Promise<GitActionResponse>,
    fallbackMessage: string,
    repositoryChanging?: boolean,
  ) => Promise<boolean>;
  setError: React.Dispatch<React.SetStateAction<string | null>>;
}

export const useProjectGitActions = ({
  client,
  projectRoot,
  summary,
  confirm,
  runAction,
  setError,
}: UseProjectGitActionsParams) => {
  const { t } = useI18n();

  const fetchRemote = useCallback(async () => {
    if (!projectRoot) return;
    await runAction(
      () => client.fetchGit({ root: projectRoot, remote: 'origin' }),
      t('git.action.fetchDone'),
    );
  }, [client, projectRoot, runAction, t]);

  const pullCurrent = useCallback(async () => {
    if (!projectRoot) return;
    await runAction(
      () => client.pullGit({ root: projectRoot, mode: 'ff-only' }),
      t('git.action.pullDone'),
      true,
    );
  }, [client, projectRoot, runAction, t]);

  const pushCurrent = useCallback(async () => {
    if (!projectRoot) return;
    await runAction(
      () => client.pushGit({ root: projectRoot }),
      t('git.action.pushDone'),
    );
  }, [client, projectRoot, runAction, t]);

  const checkoutBranch = useCallback(async (branch: GitBranchInfo) => {
    if (!projectRoot) return;
    if (branch.current) return;
    if (summary?.dirty) {
      const confirmed = await confirm({
        title: t('git.action.checkoutTitle'),
        message: t('git.action.checkoutDirtyMessage'),
        confirmText: t('git.action.checkoutConfirm'),
        cancelText: t('common.cancel'),
        type: 'warning',
      });
      if (!confirmed) return;
    }
    const isRemote = Boolean(branch.remote);
    await runAction(
      () => client.checkoutGit({
        root: projectRoot,
        branch: isRemote ? branch.shortName || branch.name.split('/').slice(1).join('/') : branch.name,
        remoteBranch: isRemote ? branch.name : undefined,
        createTracking: isRemote && !branch.trackedBy,
      }),
      t('git.action.checkoutDone', { branch: branch.shortName || branch.name }),
      true,
    );
  }, [client, confirm, projectRoot, runAction, summary?.dirty, t]);

  const mergeBranch = useCallback(async (branch: GitBranchInfo) => {
    if (!projectRoot) return;
    if (branch.current) return;
    const target = branch.name.trim();
    if (!target) return;
    if (summary?.operationState) {
      setError(t('git.action.mergeBlockedOperation', { state: summary.operationState }));
      return;
    }
    if (summary?.detached) {
      setError(t('git.action.mergeDetached'));
      return;
    }
    const current = summary?.currentBranch || 'HEAD';
    const targetLabel = branch.shortName || branch.name;
    const dirtyWarning = summary?.dirty
      ? t('git.action.mergeDirtyWarning')
      : '';
    const confirmed = await confirm({
      title: t('git.action.mergeTitle'),
      message: t('git.action.mergeMessage', {
        target: targetLabel,
        current,
        dirtyWarning,
      }),
      confirmText: t('git.action.mergeConfirm'),
      cancelText: t('common.cancel'),
      type: 'warning',
    });
    if (!confirmed) return;
    await runAction(
      () => client.mergeGit({ root: projectRoot, branch: target, mode: 'default' }),
      t('git.action.mergeDone', { target: targetLabel, current }),
      true,
    );
  }, [
    client,
    confirm,
    projectRoot,
    runAction,
    setError,
    summary?.currentBranch,
    summary?.detached,
    summary?.dirty,
    summary?.operationState,
    t,
  ]);

  const createBranch = useCallback(async (name: string, startPoint?: string) => {
    if (!projectRoot) return;
    const trimmed = name.trim();
    if (!trimmed) {
      setError(t('git.action.branchNameRequired'));
      return;
    }
    await runAction(
      () => client.createGitBranch({
        root: projectRoot,
        name: trimmed,
        startPoint,
        checkout: true,
      }),
      t('git.action.createBranchDone', { branch: trimmed }),
      true,
    );
  }, [client, projectRoot, runAction, setError, t]);

  const stageFiles = useCallback(async (paths: string[]) => {
    if (!projectRoot) return;
    const validPaths = normalizeNonEmptyPaths(paths);
    if (validPaths.length === 0) {
      setError(t('git.action.selectStageFiles'));
      return;
    }
    await runAction(
      () => client.stageGitPaths({ root: projectRoot, paths: validPaths }),
      t('git.action.stageDone'),
    );
  }, [client, projectRoot, runAction, setError, t]);

  const unstageFiles = useCallback(async (paths: string[]) => {
    if (!projectRoot) return;
    const validPaths = normalizeNonEmptyPaths(paths);
    if (validPaths.length === 0) {
      setError(t('git.action.selectUnstageFiles'));
      return;
    }
    await runAction(
      () => client.unstageGitPaths({ root: projectRoot, paths: validPaths }),
      t('git.action.unstageDone'),
    );
  }, [client, projectRoot, runAction, setError, t]);

  const discardFiles = useCallback(async (paths: string[]) => {
    if (!projectRoot) return;
    const validPaths = normalizeNonEmptyPaths(paths);
    if (validPaths.length === 0) {
      setError(t('git.action.selectDiscardFiles'));
      return;
    }
    const targetLabel = validPaths.length === 1
      ? validPaths[0]
      : t('git.action.fileCount', { count: validPaths.length });
    const confirmed = await confirm({
      title: t('git.action.discardTitle'),
      message: t('git.action.discardMessage', { target: targetLabel }),
      confirmText: t('git.action.discardConfirm'),
      cancelText: t('common.cancel'),
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }
    await runAction(
      () => client.discardGitPaths({ root: projectRoot, paths: validPaths }),
      t('git.action.discardDone'),
    );
  }, [client, confirm, projectRoot, runAction, setError, t]);

  const commitStaged = useCallback(async (message: string) => {
    if (!projectRoot) return false;
    const trimmed = message.trim();
    if (!trimmed) {
      setError(t('git.action.commitMessageRequired'));
      return false;
    }
    return runAction(
      () => client.commitGit({ root: projectRoot, message: trimmed }),
      t('git.action.commitStagedDone'),
      true,
    );
  }, [client, projectRoot, runAction, setError, t]);

  const commitSelected = useCallback(async (message: string, paths: string[]) => {
    if (!projectRoot) return false;
    const trimmed = message.trim();
    if (!trimmed) {
      setError(t('git.action.commitMessageRequired'));
      return false;
    }
    if (paths.length === 0) {
      setError(t('git.action.selectCommitFiles'));
      return false;
    }
    return runAction(
      () => client.commitGit({ root: projectRoot, message: trimmed, paths }),
      t('git.action.commitDone'),
      true,
    );
  }, [client, projectRoot, runAction, setError, t]);

  return {
    fetchRemote,
    pullCurrent,
    pushCurrent,
    checkoutBranch,
    mergeBranch,
    createBranch,
    stageFiles,
    unstageFiles,
    discardFiles,
    commitStaged,
    commitSelected,
  };
};
