import { useCallback } from 'react';

import type { GitActionResponse } from '../../../lib/api/client/types';
import type { GitBranchInfo } from '../../../types';
import type { ProjectGitApiClient } from './projectGitTypes';
import { normalizeNonEmptyPaths } from './projectGitHelpers';

interface UseProjectGitActionsParams {
  client: ProjectGitApiClient;
  projectRoot: string;
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
  const fetchRemote = useCallback(async () => {
    await runAction(
      () => client.fetchGit({ root: projectRoot, remote: 'origin' }),
      'Fetch 完成',
    );
  }, [client, projectRoot, runAction]);

  const pullCurrent = useCallback(async () => {
    await runAction(
      () => client.pullGit({ root: projectRoot, mode: 'ff-only' }),
      'Pull 完成',
      true,
    );
  }, [client, projectRoot, runAction]);

  const pushCurrent = useCallback(async () => {
    await runAction(
      () => client.pushGit({ root: projectRoot }),
      'Push 完成',
    );
  }, [client, projectRoot, runAction]);

  const checkoutBranch = useCallback(async (branch: GitBranchInfo) => {
    if (branch.current) return;
    if (summary?.dirty) {
      const confirmed = await confirm({
        title: '切换分支',
        message: '当前工作区有未提交改动，切换分支可能失败或影响改动。是否继续？',
        confirmText: '继续切换',
        cancelText: '取消',
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
      `已切换到 ${branch.shortName || branch.name}`,
      true,
    );
  }, [client, confirm, projectRoot, runAction, summary?.dirty]);

  const mergeBranch = useCallback(async (branch: GitBranchInfo) => {
    if (branch.current) return;
    const target = branch.name.trim();
    if (!target) return;
    if (summary?.operationState) {
      setError(`当前处于 ${summary.operationState} 状态，请先处理完再 Merge`);
      return;
    }
    if (summary?.detached) {
      setError('当前是 detached HEAD，无法从界面执行 Merge');
      return;
    }
    const current = summary?.currentBranch || 'HEAD';
    const targetLabel = branch.shortName || branch.name;
    const dirtyWarning = summary?.dirty
      ? '\n\n当前工作区有未提交改动，Merge 可能失败或产生冲突。建议先提交或暂存。是否继续？'
      : '';
    const confirmed = await confirm({
      title: '合并分支',
      message: `确认将 ${targetLabel} 合并到当前分支 ${current} 吗？${dirtyWarning}`,
      confirmText: '确认合并',
      cancelText: '取消',
      type: 'warning',
    });
    if (!confirmed) return;
    await runAction(
      () => client.mergeGit({ root: projectRoot, branch: target, mode: 'default' }),
      `已将 ${targetLabel} 合并到 ${current}`,
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
  ]);

  const createBranch = useCallback(async (name: string, startPoint?: string) => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError('分支名不能为空');
      return;
    }
    await runAction(
      () => client.createGitBranch({
        root: projectRoot,
        name: trimmed,
        startPoint,
        checkout: true,
      }),
      `已创建并切换到 ${trimmed}`,
      true,
    );
  }, [client, projectRoot, runAction, setError]);

  const stageFiles = useCallback(async (paths: string[]) => {
    const validPaths = normalizeNonEmptyPaths(paths);
    if (validPaths.length === 0) {
      setError('请选择要 Stage 的文件');
      return;
    }
    await runAction(
      () => client.stageGitPaths({ root: projectRoot, paths: validPaths }),
      'Stage 完成',
    );
  }, [client, projectRoot, runAction, setError]);

  const unstageFiles = useCallback(async (paths: string[]) => {
    const validPaths = normalizeNonEmptyPaths(paths);
    if (validPaths.length === 0) {
      setError('请选择要 Unstage 的文件');
      return;
    }
    await runAction(
      () => client.unstageGitPaths({ root: projectRoot, paths: validPaths }),
      'Unstage 完成',
    );
  }, [client, projectRoot, runAction, setError]);

  const discardFiles = useCallback(async (paths: string[]) => {
    const validPaths = normalizeNonEmptyPaths(paths);
    if (validPaths.length === 0) {
      setError('请选择要回滚的文件');
      return;
    }
    const targetLabel = validPaths.length === 1
      ? validPaths[0]
      : `${validPaths.length} 个文件`;
    const confirmed = await confirm({
      title: '取消变更',
      message: `确认回滚 ${targetLabel} 的变更吗？\n\n已跟踪文件会恢复到 HEAD；未跟踪文件会被删除。该操作不可撤销。`,
      confirmText: '确认回滚',
      cancelText: '取消',
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }
    await runAction(
      () => client.discardGitPaths({ root: projectRoot, paths: validPaths }),
      '回滚完成',
    );
  }, [client, confirm, projectRoot, runAction, setError]);

  const commitStaged = useCallback(async (message: string) => {
    const trimmed = message.trim();
    if (!trimmed) {
      setError('Commit message 不能为空');
      return false;
    }
    return runAction(
      () => client.commitGit({ root: projectRoot, message: trimmed }),
      'Commit staged 完成',
      true,
    );
  }, [client, projectRoot, runAction, setError]);

  const commitSelected = useCallback(async (message: string, paths: string[]) => {
    const trimmed = message.trim();
    if (!trimmed) {
      setError('Commit message 不能为空');
      return false;
    }
    if (paths.length === 0) {
      setError('请至少选择一个文件');
      return false;
    }
    return runAction(
      () => client.commitGit({ root: projectRoot, message: trimmed, paths }),
      'Commit 完成',
      true,
    );
  }, [client, projectRoot, runAction, setError]);

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
