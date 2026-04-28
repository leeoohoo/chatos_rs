import { useEffect, useState } from 'react';

import type ApiClient from '../../../lib/api/client';
import { ApiRequestError } from '../../../lib/api/client/shared';
import { normalizeTerminal } from '../../../lib/domain/terminals';
import {
  resolveProjectRuntimeTerminal,
  RUNNER_START_COMMAND,
} from '../../../lib/domain/projectRunner';
import type { ProjectRunnerActiveTerminal } from '../../../lib/domain/projectRunner';
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
  const [activeRun, setActiveRun] = useState<ProjectRunnerActiveTerminal | null>(null);
  const [activeTerminalBusy, setActiveTerminalBusy] = useState(false);

  const resetActiveRunState = () => {
    setActiveRun(null);
    setActiveTerminalBusy(false);
  };

  useEffect(() => {
    if (!project?.id) {
      resetActiveRunState();
      return;
    }

    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const poll = async () => {
      try {
        const list = await client.listTerminals();
        if (disposed || !Array.isArray(list)) {
          return;
        }
        const normalizedTerminals = list.map((item) => normalizeTerminal(item));
        const { busyTerminal, activeTerminal } = resolveProjectRuntimeTerminal(
          normalizedTerminals,
          project.id,
        );
        const chosen = activeTerminal;
        if (!chosen) {
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
        // ignore polling errors
      } finally {
        if (!disposed) {
          timer = setTimeout(() => {
            void poll();
          }, 2000);
        }
      }
    };
    void poll();
    return () => {
      disposed = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
  }, [client, project?.id, project?.rootPath]);

  useEffect(() => {
    if (!activeRun?.terminalId) {
      setActiveTerminalBusy(false);
      return;
    }

    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const poll = async () => {
      try {
        const terminal = await client.getTerminal(activeRun.terminalId);
        if (disposed) {
          return;
        }
        setActiveTerminalBusy(Boolean(terminal?.busy));
      } catch (error) {
        if (!disposed) {
          setActiveTerminalBusy(false);
          if (error instanceof ApiRequestError && error.status === 404) {
            setActiveRun(null);
          }
        }
      } finally {
        if (!disposed) {
          timer = setTimeout(() => {
            void poll();
          }, 1500);
        }
      }
    };
    void poll();
    return () => {
      disposed = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
  }, [activeRun?.terminalId, client]);

  return {
    activeRun,
    activeTerminalBusy,
    setActiveRun,
    setActiveTerminalBusy,
    resetActiveRunState,
  };
};
