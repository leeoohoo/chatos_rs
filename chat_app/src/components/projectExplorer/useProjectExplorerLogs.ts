import { useCallback, useEffect, useMemo, useState } from 'react';
import type { ProjectChangeLogResponse } from '../../lib/api/client/types';
import type { ChangeLogItem } from '../../types';
import { normalizeChangeLog } from './utils';

interface ProjectExplorerLogsClient {
  listProjectChangeLogs(
    projectId: string,
    params?: { path?: string; limit?: number; offset?: number },
  ): Promise<ProjectChangeLogResponse[]>;
}

interface Params {
  client: ProjectExplorerLogsClient;
  projectId?: string;
  selectedPath: string | null;
  selectedFilePath: string | null;
}

const readErrorMessage = (error: unknown, fallback: string): string => (
  error instanceof Error ? error.message : fallback
);

export const useProjectExplorerLogs = ({
  client,
  projectId,
  selectedPath,
  selectedFilePath,
}: Params) => {
  const [changeLogs, setChangeLogs] = useState<ChangeLogItem[]>([]);
  const [loadingLogs, setLoadingLogs] = useState(false);
  const [logsError, setLogsError] = useState<string | null>(null);
  const [selectedLogId, setSelectedLogId] = useState<string | null>(null);

  const resetLogsState = useCallback(() => {
    setChangeLogs([]);
    setLogsError(null);
    setSelectedLogId(null);
  }, []);

  useEffect(() => {
    const logPath = selectedFilePath || selectedPath;
    if (!projectId || !logPath) {
      resetLogsState();
      return;
    }
    let cancelled = false;
    const loadLogs = async () => {
      setLoadingLogs(true);
      setLogsError(null);
      try {
        const list = await client.listProjectChangeLogs(projectId, { path: logPath, limit: 100 });
        if (!cancelled) {
          const normalized = Array.isArray(list) ? list.map(normalizeChangeLog) : [];
          setChangeLogs(normalized);
        }
      } catch (err) {
        if (!cancelled) {
          setLogsError(readErrorMessage(err, '加载变更记录失败'));
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
  }, [client, projectId, resetLogsState, selectedFilePath, selectedPath]);

  useEffect(() => {
    if (selectedLogId && !changeLogs.find(log => log.id === selectedLogId)) {
      setSelectedLogId(null);
    }
  }, [changeLogs, selectedLogId]);

  const selectedLog = useMemo(
    () => (selectedLogId ? changeLogs.find(log => log.id === selectedLogId) || null : null),
    [changeLogs, selectedLogId]
  );

  return {
    changeLogs,
    loadingLogs,
    logsError,
    selectedLogId,
    setSelectedLogId,
    selectedLog,
    resetLogsState,
  };
};
