import { useCallback, useEffect, useState } from 'react';

import type ApiClient from '../../../lib/api/client';
import { normalizeTerminal } from '../../../lib/domain/terminals';
import {
  resolveProjectRuntimeTerminal,
  RUNNER_START_COMMAND,
} from '../../../lib/domain/projectRunner';
import { useChatRuntimeEnv } from '../../../lib/store/ChatStoreContext';
import { loadTerminalsSnapshot } from '../../../lib/store/actions/terminalsCache';
import type { ProjectRunnerActiveTerminal } from '../../../lib/domain/projectRunner';
import { useProjectRunRealtime } from '../../../lib/realtime/useProjectRunRealtime';
import { useTerminalStateRealtime } from '../../../lib/realtime/useTerminalStateRealtime';
import type { Project } from '../../../types';

const readTrimmedString = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');

interface UseProjectRunnerTerminalPollingOptions {
  client: ApiClient;
  project: Project | null;
}

export const useProjectRunnerTerminalPolling = ({
  client,
  project,
}: UseProjectRunnerTerminalPollingOptions) => {
  const { userId } = useChatRuntimeEnv();
  const [activeRun, setActiveRun] = useState<ProjectRunnerActiveTerminal | null>(null);
  const [activeTerminalBusy, setActiveTerminalBusy] = useState(false);

  const resetActiveRunState = useCallback(() => {
    setActiveRun(null);
    setActiveTerminalBusy(false);
  }, []);

  const refreshProjectActiveRun = useCallback(async () => {
    if (!project?.id) {
      resetActiveRunState();
      return;
    }
    try {
      const list = await loadTerminalsSnapshot(client, userId);
      if (!Array.isArray(list)) {
        return;
      }
      const normalizedTerminals = list.map((item) => normalizeTerminal(item));
      const { busyTerminal, activeTerminal } = resolveProjectRuntimeTerminal(
        normalizedTerminals,
        project.id,
      );
      const chosen = activeTerminal;
      if (!chosen) {
        setActiveRun(null);
        setActiveTerminalBusy(false);
        return;
      }

      const terminalId = readTrimmedString(chosen.id);
      const terminalName = readTrimmedString(chosen.name) || terminalId;
      setActiveTerminalBusy(Boolean(busyTerminal?.busy || chosen.busy));
      if (!terminalId) {
        return;
      }
      setActiveRun((prev) => ({
        terminalId,
        terminalName,
        command: prev?.command || RUNNER_START_COMMAND,
        cwd: readTrimmedString(chosen.cwd) || prev?.cwd || readTrimmedString(project.rootPath || ''),
        dispatchedAt: prev?.dispatchedAt || Date.now(),
      }));
    } catch {
      // ignore refresh errors
    }
  }, [client, project?.id, project?.rootPath, resetActiveRunState, userId]);

  useEffect(() => {
    void refreshProjectActiveRun();
  }, [refreshProjectActiveRun]);

  useProjectRunRealtime({
    enabled: Boolean(project?.id),
    projectId: project?.id || null,
    onRunStateChanged: async (payload) => {
      const terminalId = readTrimmedString(payload.terminal_id);
      const terminalName = readTrimmedString(payload.terminal_name) || terminalId;
      const cwd = readTrimmedString(payload.cwd) || readTrimmedString(project?.rootPath || '');
      setActiveTerminalBusy(Boolean(payload.busy));

      if ((readTrimmedString(payload.status) || '') === 'exited') {
        setActiveRun(null);
        setActiveTerminalBusy(false);
        return;
      }

      if (!terminalId) {
        if (payload.running !== true) {
          setActiveRun(null);
          setActiveTerminalBusy(false);
        }
        return;
      }

      setActiveRun((prev) => ({
        terminalId,
        terminalName: terminalName || prev?.terminalName || terminalId,
        command: prev?.command || RUNNER_START_COMMAND,
        cwd: cwd || prev?.cwd || '',
        dispatchedAt: prev?.dispatchedAt || Date.now(),
      }));
    },
  });

  useTerminalStateRealtime({
    enabled: Boolean(activeRun?.terminalId),
    terminalId: activeRun?.terminalId || null,
    onStateChanged: async (payload) => {
      setActiveTerminalBusy(Boolean(payload.busy));
      if (payload.status === 'exited') {
        setActiveRun(null);
        setActiveTerminalBusy(false);
        return;
      }
      setActiveRun((prev) => {
        if (!prev) {
          return prev;
        }
        const nextTerminalName = readTrimmedString(payload.terminal_name) || prev.terminalName;
        const nextCwd = readTrimmedString(payload.cwd) || prev.cwd;
        if (nextTerminalName === prev.terminalName && nextCwd === prev.cwd) {
          return prev;
        }
        return {
          ...prev,
          terminalName: nextTerminalName,
          cwd: nextCwd,
        };
      });
    },
  });

  return {
    activeRun,
    activeTerminalBusy,
    setActiveRun,
    setActiveTerminalBusy,
    resetActiveRunState,
  };
};
