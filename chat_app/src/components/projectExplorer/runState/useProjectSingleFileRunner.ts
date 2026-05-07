import { useCallback } from 'react';

import type ApiClient from '../../../lib/api/client';
import type { FsEntry, Project } from '../../../types';
import { buildSingleFileRunProfile } from '../runProfiles';

const readTrimmedString = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');

interface UseProjectSingleFileRunnerOptions {
  client: ApiClient;
  project: Project | null;
  setActionError: (value: string | null) => void;
  setActionLoading: (value: boolean) => void;
  setActionMessage: (value: string | null) => void;
}

export const useProjectSingleFileRunner = ({
  client,
  project,
  setActionError,
  setActionLoading,
  setActionMessage,
}: UseProjectSingleFileRunnerOptions) => {
  const canRunFile = useCallback((entry: FsEntry) => {
    if (entry.isDir) {
      return false;
    }
    return Boolean(buildSingleFileRunProfile(entry.path));
  }, []);

  const handleRunFile = useCallback(async (entry: FsEntry) => {
    const profile = buildSingleFileRunProfile(entry.path);
    if (!profile) {
      setActionError('该文件类型暂不支持直接运行');
      return;
    }
    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const result = await client.dispatchTerminalCommand({
        cwd: profile.cwd,
        command: profile.command,
        project_id: project?.id,
        create_if_missing: true,
      });
      const terminalName = readTrimmedString(
        (result as { terminal_name?: string | null; terminal_id?: string | null })?.terminal_name
        || (result as { terminal_name?: string | null; terminal_id?: string | null })?.terminal_id,
      );
      setActionMessage(terminalName ? `已在终端 ${terminalName} 运行文件` : '已派发运行命令');
    } catch (error) {
      setActionError(error instanceof Error ? error.message : '运行文件失败');
    } finally {
      setActionLoading(false);
    }
  }, [client, project?.id, setActionError, setActionLoading, setActionMessage]);

  return {
    canRunFile,
    handleRunFile,
  };
};
