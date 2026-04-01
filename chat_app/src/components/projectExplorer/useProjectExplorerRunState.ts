import { useCallback, useEffect, useMemo, useState } from 'react';
import type ApiClient from '../../lib/api/client';
import type {
  FsEntry,
  Project,
  ProjectRunTarget,
} from '../../types';
import { buildSingleFileRunProfile } from './runProfiles';
import { normalizeProjectRunCatalog } from './utils';

interface UseProjectExplorerRunStateParams {
  client: ApiClient;
  project: Project | null;
  selectedEntry: FsEntry | null;
  selectedPath: string | null;
  getParentPath: (path: string | null | undefined) => string;
  setActionError: (value: string | null) => void;
  setActionLoading: (value: boolean) => void;
  setActionMessage: (value: string | null) => void;
}

export const useProjectExplorerRunState = ({
  client,
  project,
  selectedEntry,
  selectedPath,
  getParentPath,
  setActionError,
  setActionLoading,
  setActionMessage,
}: UseProjectExplorerRunStateParams) => {
  const [runStatus, setRunStatus] = useState<string>('analyzing');
  const [runTargets, setRunTargets] = useState<ProjectRunTarget[]>([]);
  const [runCatalogLoading, setRunCatalogLoading] = useState(false);
  const [runCatalogError, setRunCatalogError] = useState<string | null>(null);
  const [selectedRunTargetId, setSelectedRunTargetId] = useState<string | null>(null);

  const runCwd = useMemo(() => {
    if (!project?.rootPath) {
      return '';
    }
    if (selectedEntry?.isDir) {
      return selectedEntry.path;
    }
    if (selectedEntry && !selectedEntry.isDir) {
      return getParentPath(selectedEntry.path) || project.rootPath;
    }
    if (selectedPath) {
      return getParentPath(selectedPath) || project.rootPath;
    }
    return project.rootPath;
  }, [getParentPath, project?.rootPath, selectedEntry, selectedPath]);

  const handleDispatchTerminalCommand = useCallback(async (payload: { cwd: string; command: string }) => {
    return client.dispatchTerminalCommand({
      cwd: payload.cwd,
      command: payload.command,
      project_id: project?.id,
      create_if_missing: true,
    });
  }, [client, project?.id]);

  const handleInterruptTerminal = useCallback(async (terminalId: string, payload?: { reason?: string }) => {
    return client.interruptTerminal(terminalId, payload);
  }, [client]);

  const handleGetTerminal = useCallback(async (terminalId: string) => {
    return client.getTerminal(terminalId);
  }, [client]);

  const handleListTerminalLogs = useCallback(async (
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string },
  ) => {
    return client.listTerminalLogs(terminalId, params);
  }, [client]);

  const handleListTerminals = useCallback(async () => {
    return client.listTerminals();
  }, [client]);

  const loadRunCatalog = useCallback(async (analyze = false) => {
    if (!project?.id) {
      return;
    }
    setRunCatalogLoading(true);
    setRunCatalogError(null);
    try {
      const raw = analyze
        ? await client.analyzeProjectRun(project.id)
        : await client.getProjectRunCatalog(project.id);
      const catalog = normalizeProjectRunCatalog(raw);
      setRunStatus(catalog.status || (catalog.targets.length > 0 ? 'ready' : 'empty'));
      setRunTargets(catalog.targets || []);
      const nextDefault = catalog.defaultTargetId
        ? String(catalog.defaultTargetId)
        : (catalog.targets.find((item) => item.isDefault)?.id || null);
      setSelectedRunTargetId(nextDefault);
    } catch (error) {
      setRunStatus('error');
      setRunTargets([]);
      setSelectedRunTargetId(null);
      setRunCatalogError(error instanceof Error ? error.message : '运行目标分析失败');
    } finally {
      setRunCatalogLoading(false);
    }
  }, [client, project?.id]);

  useEffect(() => {
    setRunStatus('analyzing');
    setRunTargets([]);
    setSelectedRunTargetId(null);
    setRunCatalogError(null);
    if (!project?.id) {
      return;
    }
    void loadRunCatalog(true);
  }, [loadRunCatalog, project?.id]);

  const handleAnalyzeRunTargets = useCallback(() => {
    void loadRunCatalog(true);
  }, [loadRunCatalog]);

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
      const terminalName = String(
        (result as { terminal_name?: string | null; terminal_id?: string | null })?.terminal_name
        || (result as { terminal_name?: string | null; terminal_id?: string | null })?.terminal_id
        || '',
      );
      setActionMessage(terminalName ? `已在终端 ${terminalName} 运行文件` : '已派发运行命令');
    } catch (error) {
      setActionError(error instanceof Error ? error.message : '运行文件失败');
    } finally {
      setActionLoading(false);
    }
  }, [client, project?.id, setActionError, setActionLoading, setActionMessage]);

  return {
    runCwd,
    runStatus,
    runTargets,
    runCatalogLoading,
    runCatalogError,
    selectedRunTargetId,
    setSelectedRunTargetId,
    handleDispatchTerminalCommand,
    handleInterruptTerminal,
    handleGetTerminal,
    handleListTerminalLogs,
    handleListTerminals,
    handleAnalyzeRunTargets,
    canRunFile,
    handleRunFile,
  };
};
