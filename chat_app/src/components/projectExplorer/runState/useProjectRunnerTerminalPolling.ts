import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type ApiClient from '../../../lib/api/client';
import { normalizeProjectRunState } from '../../../lib/domain/projectExplorer';
import { useProjectRunRealtime } from '../../../lib/realtime/useProjectRunRealtime';
import { useTerminalStateRealtime } from '../../../lib/realtime/useTerminalStateRealtime';
import type {
  Project,
  ProjectRunInstance,
  ProjectRunState,
} from '../../../types';
import type { ProjectRunnerActiveTerminal } from '../../../lib/domain/projectRunner';

const readTrimmedString = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');

const toActiveRun = (
  instance: ProjectRunInstance | null | undefined,
  previous?: ProjectRunnerActiveTerminal | null,
): ProjectRunnerActiveTerminal | null => {
  if (!instance?.terminalId) {
    return null;
  }
  return {
    terminalId: instance.terminalId,
    terminalName: instance.terminalName || previous?.terminalName || instance.terminalId,
    command: previous?.command || '',
    cwd: instance.cwd || previous?.cwd || '',
    dispatchedAt: previous?.dispatchedAt || Date.now(),
    origin: previous?.origin || 'discovered',
    exitCode: previous?.exitCode ?? null,
    exitReason: previous?.exitReason ?? null,
  };
};

interface UseProjectRunnerTerminalPollingOptions {
  client: ApiClient;
  project: Project | null;
}

export const useProjectRunnerTerminalPolling = ({
  client,
  project,
}: UseProjectRunnerTerminalPollingOptions) => {
  const [projectRunState, setProjectRunState] = useState<ProjectRunState | null>(null);
  const [selectedRunInstanceId, setSelectedRunInstanceId] = useState<string | null>(null);
  const [activeRun, setActiveRun] = useState<ProjectRunnerActiveTerminal | null>(null);
  const [lastExitedRun, setLastExitedRun] = useState<ProjectRunnerActiveTerminal | null>(null);
  const [activeTerminalBusy, setActiveTerminalBusy] = useState(false);
  const activeRunRef = useRef<ProjectRunnerActiveTerminal | null>(null);

  useEffect(() => {
    activeRunRef.current = activeRun;
  }, [activeRun]);

  const resetActiveRunState = useCallback(() => {
    setProjectRunState(null);
    setSelectedRunInstanceId(null);
    setActiveRun(null);
    setLastExitedRun(null);
    setActiveTerminalBusy(false);
  }, []);

  const removeRunInstanceLocally = useCallback((
    terminalId: string,
    nextSelectedTerminalId?: string | null,
  ) => {
    const normalizedTerminalId = readTrimmedString(terminalId);
    const normalizedNextSelectedTerminalId = nextSelectedTerminalId === undefined
      ? undefined
      : readTrimmedString(nextSelectedTerminalId) || null;
    if (!normalizedTerminalId) {
      return;
    }

    setProjectRunState((prev) => {
      if (!prev) {
        return prev;
      }
      const nextInstances = (prev.instances || []).filter((item) => item.terminalId !== normalizedTerminalId);
      const nextSelectedInstance = normalizedNextSelectedTerminalId === undefined
        ? (nextInstances[0] || null)
        : (normalizedNextSelectedTerminalId
          ? nextInstances.find((item) => item.terminalId === normalizedNextSelectedTerminalId) || null
          : null);
      return {
        ...prev,
        running: nextInstances.some((item) => item.running),
        busy: nextInstances.some((item) => item.busy),
        status: nextInstances.some((item) => item.running) ? 'running' : (nextInstances[0]?.status || 'idle'),
        terminalId: nextSelectedInstance?.terminalId || null,
        terminalName: nextSelectedInstance?.terminalName || null,
        cwd: nextSelectedInstance?.cwd || null,
        terminal: nextSelectedInstance?.terminal || null,
        instances: nextInstances,
      };
    });
    setSelectedRunInstanceId(normalizedNextSelectedTerminalId === undefined ? null : normalizedNextSelectedTerminalId);
    setLastExitedRun((prev) => (prev?.terminalId === normalizedTerminalId ? null : prev));
  }, []);

  const refreshProjectActiveRun = useCallback(async () => {
    if (!project?.id) {
      resetActiveRunState();
      return;
    }
    try {
      const raw = await client.getProjectRunState(project.id);
      const state = normalizeProjectRunState(raw);
      setProjectRunState(state);
      setSelectedRunInstanceId((prev) => {
        if (prev && state.instances?.some((item) => item.terminalId === prev)) {
          return prev;
        }
        return state.instances?.[0]?.terminalId || state.terminalId || null;
      });
    } catch {
      // ignore refresh errors
    }
  }, [client, project?.id, resetActiveRunState]);

  useEffect(() => {
    void refreshProjectActiveRun();
  }, [refreshProjectActiveRun]);

  const projectRunInstances = useMemo(
    () => projectRunState?.instances || [],
    [projectRunState?.instances],
  );

  const selectedInstance = useMemo(() => {
    if (!projectRunInstances.length) {
      return null;
    }
    return projectRunInstances.find((item) => item.terminalId === selectedRunInstanceId)
      || projectRunInstances[0]
      || null;
  }, [projectRunInstances, selectedRunInstanceId]);

  useEffect(() => {
    setActiveRun((prev) => toActiveRun(selectedInstance, prev));
    setActiveTerminalBusy(Boolean(selectedInstance?.busy));
  }, [selectedInstance]);

  const recordExitedRun = useCallback((terminalId: string, reason?: string | null, exitCode?: number | null) => {
    const current = activeRunRef.current;
    if (current?.terminalId === terminalId) {
      setLastExitedRun({
        ...current,
        exitReason: reason ?? current.exitReason ?? null,
        exitCode: exitCode ?? current.exitCode ?? null,
      });
    }
  }, []);

  useProjectRunRealtime({
    enabled: Boolean(project?.id),
    projectId: project?.id || null,
    onRunStateChanged: async () => {
      await refreshProjectActiveRun();
    },
    onRunInstanceChanged: async (payload) => {
      const changedTerminalId = readTrimmedString(payload.terminal_id);
      if ((readTrimmedString(payload.status) || '') === 'exited' && changedTerminalId) {
        recordExitedRun(
          changedTerminalId,
          readTrimmedString(payload.reason) || null,
          typeof payload.exit_code === 'number' ? payload.exit_code : null,
        );
      }
      await refreshProjectActiveRun();
    },
  });

  useTerminalStateRealtime({
    enabled: Boolean(selectedInstance?.terminalId),
    terminalId: selectedInstance?.terminalId || null,
    onStateChanged: async (payload) => {
      const nextStatus = readTrimmedString(payload.status) || 'idle';
      const nextBusy = Boolean(payload.busy);
      setProjectRunState((prev) => {
        if (!prev) {
          return prev;
        }
        const nextInstances = (prev.instances || []).map((item) => {
          if (item.terminalId !== selectedInstance?.terminalId) {
            return item;
          }
          return {
            ...item,
            status: nextStatus,
            busy: nextBusy,
            running: nextStatus === 'running',
            cwd: readTrimmedString(payload.cwd) || item.cwd,
            terminal: item.terminal
              ? {
                ...item.terminal,
                status: nextStatus,
                busy: nextBusy,
                cwd: readTrimmedString(payload.cwd) || item.terminal.cwd,
                name: readTrimmedString(payload.terminal_name) || item.terminal.name,
              }
              : item.terminal,
          };
        });
        const selected = nextInstances.find((item) => item.terminalId === selectedInstance?.terminalId) || null;
        return {
          ...prev,
          running: nextInstances.some((item) => item.running),
          busy: nextInstances.some((item) => item.busy),
          status: nextInstances.some((item) => item.running) ? 'running' : (nextInstances[0]?.status || 'idle'),
          terminalId: selected?.terminalId || prev.terminalId,
          terminalName: selected?.terminalName || prev.terminalName,
          cwd: selected?.cwd || prev.cwd,
          terminal: selected?.terminal || prev.terminal,
          instances: nextInstances,
        };
      });
      setActiveTerminalBusy(nextBusy);
      if (nextStatus === 'exited' && selectedInstance?.terminalId) {
        recordExitedRun(
          selectedInstance.terminalId,
          readTrimmedString(payload.reason) || null,
          typeof payload.exit_code === 'number' ? payload.exit_code : null,
        );
      }
    },
  });

  return {
    projectRunState,
    projectRunInstances,
    selectedRunInstanceId,
    activeRun,
    lastExitedRun,
    activeTerminalBusy,
    projectRunTerminal: selectedInstance?.terminal || null,
    selectRunInstance: setSelectedRunInstanceId,
    setActiveRun,
    setLastExitedRun,
    setActiveTerminalBusy,
    resetActiveRunState,
    removeRunInstanceLocally,
    refreshProjectActiveRun,
  };
};
