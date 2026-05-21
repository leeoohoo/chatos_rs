import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type ApiClient from '../../../lib/api/client';
import { normalizeProjectRunState } from '../../../lib/domain/projectExplorer';
import { useProjectRunRealtime } from '../../../lib/realtime/useProjectRunRealtime';
import { useTerminalStateRealtime } from '../../../lib/realtime/useTerminalStateRealtime';
import type {
  Project,
  ProjectRunState,
} from '../../../types';
import type { ProjectRunnerActiveTerminal } from '../../../lib/domain/projectRunner';
import {
  applyProjectRunnerTerminalStatePayload,
  buildProjectRunnerActiveRun,
  removeProjectRunnerTerminalInstance,
  resolveProjectRunnerSelectedInstance,
} from './projectRunnerTerminalState';

const readTrimmedString = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');

interface UseProjectRunnerTerminalPollingOptions {
  client: ApiClient;
  project: Project | null;
  enabled?: boolean;
}

export const useProjectRunnerTerminalPolling = ({
  client,
  project,
  enabled = true,
}: UseProjectRunnerTerminalPollingOptions) => {
  const [projectRunState, setProjectRunState] = useState<ProjectRunState | null>(null);
  const [selectedRunInstanceId, setSelectedRunInstanceId] = useState<string | null>(null);
  const [activeRun, setActiveRun] = useState<ProjectRunnerActiveTerminal | null>(null);
  const [lastExitedRun, setLastExitedRun] = useState<ProjectRunnerActiveTerminal | null>(null);
  const [activeTerminalBusy, setActiveTerminalBusy] = useState(false);
  const activeRunRef = useRef<ProjectRunnerActiveTerminal | null>(null);
  const runStateRequestVersionRef = useRef(0);
  const activeProjectKeyRef = useRef<string | null>(null);

  useEffect(() => {
    activeRunRef.current = activeRun;
  }, [activeRun]);

  const resetActiveRunState = useCallback(() => {
    runStateRequestVersionRef.current += 1;
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

    setProjectRunState((prev) => removeProjectRunnerTerminalInstance({
      state: prev,
      terminalId: normalizedTerminalId,
      nextSelectedTerminalId: normalizedNextSelectedTerminalId,
    }).nextState);
    setSelectedRunInstanceId(normalizedNextSelectedTerminalId === undefined ? null : normalizedNextSelectedTerminalId);
    setLastExitedRun((prev) => (prev?.terminalId === normalizedTerminalId ? null : prev));
  }, []);

  const refreshProjectActiveRun = useCallback(async () => {
    if (!project?.id) {
      resetActiveRunState();
      return;
    }
    if (!enabled) {
      return;
    }
    const projectId = project.id;
    const requestVersion = ++runStateRequestVersionRef.current;
    try {
      const raw = await client.getProjectRunState(projectId);
      if (
        runStateRequestVersionRef.current !== requestVersion
        || !enabled
        || project?.id !== projectId
      ) {
        return;
      }
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
  }, [client, enabled, project?.id, resetActiveRunState]);

  useEffect(() => {
    const nextProjectKey = enabled ? project?.id || null : null;
    if (activeProjectKeyRef.current === nextProjectKey) {
      return;
    }

    activeProjectKeyRef.current = nextProjectKey;
    resetActiveRunState();
    if (nextProjectKey) {
      void refreshProjectActiveRun();
    }
  }, [enabled, project?.id, refreshProjectActiveRun, resetActiveRunState]);

  const projectRunInstances = useMemo(
    () => projectRunState?.instances || [],
    [projectRunState?.instances],
  );

  const selectedInstance = useMemo(() => {
    return resolveProjectRunnerSelectedInstance(projectRunInstances, selectedRunInstanceId);
  }, [projectRunInstances, selectedRunInstanceId]);

  useEffect(() => {
    setActiveRun((prev) => buildProjectRunnerActiveRun(selectedInstance, prev));
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
    enabled: enabled && Boolean(project?.id),
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
    enabled: enabled && Boolean(selectedInstance?.terminalId),
    terminalId: selectedInstance?.terminalId || null,
    onStateChanged: async (payload) => {
      const nextStatus = readTrimmedString(payload.status) || 'idle';
      const nextBusy = Boolean(payload.busy);
      setProjectRunState((prev) => applyProjectRunnerTerminalStatePayload({
        state: prev,
        selectedRunInstanceId: selectedInstance?.terminalId || null,
        payload,
      }));
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
