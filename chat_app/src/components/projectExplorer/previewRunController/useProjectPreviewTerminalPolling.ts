import { useEffect } from 'react';

import type { TerminalResponse } from '../../../lib/api/client/types';
import type { Terminal } from '../../../types';
import { useProjectRunRealtime } from '../../../lib/realtime/useProjectRunRealtime';
import { useTerminalStateRealtime } from '../../../lib/realtime/useTerminalStateRealtime';
import type {
  ProjectPreviewRunSetter,
} from './previewRunControllerTypes';

const readProjectId = (item: TerminalResponse | Terminal): string => {
  const raw = item as TerminalResponse;
  return String(item?.projectId || raw?.project_id || '').trim();
};

const readLastActiveAt = (item: TerminalResponse | Terminal): string | Date | number => {
  const raw = item as TerminalResponse;
  return item?.lastActiveAt || raw?.last_active_at || 0;
};

interface UseProjectPreviewTerminalPollingOptions {
  activeRunTerminalId: string | null;
  currentCommand: string;
  projectId: string;
  projectRootPath: string;
  runTargetCwd: string;
  selectedRunTargetCommand: string | undefined;
  onListTerminals: () => Promise<Array<TerminalResponse | Terminal>>;
  setActiveRun: ProjectPreviewRunSetter;
  setActiveTerminalBusy: (value: boolean) => void;
}

export const useProjectPreviewTerminalPolling = ({
  activeRunTerminalId,
  currentCommand,
  projectId,
  projectRootPath,
  runTargetCwd,
  selectedRunTargetCommand,
  onListTerminals,
  setActiveRun,
  setActiveTerminalBusy,
}: UseProjectPreviewTerminalPollingOptions) => {
  useEffect(() => {
    if (!projectId) {
      return;
    }

    let disposed = false;
    const refresh = async () => {
      try {
        const list = await onListTerminals();
        if (disposed || !Array.isArray(list)) {
          return;
        }
        const related = list
          .filter((item) => readProjectId(item) === projectId)
          .sort((a, b) => {
            const ta = new Date(readLastActiveAt(a)).getTime();
            const tb = new Date(readLastActiveAt(b)).getTime();
            return tb - ta;
          });
        const busy = related.find((item) => Boolean(item?.busy));
        const chosen = busy || related[0] || null;
        if (!chosen) {
          setActiveRun(null);
          setActiveTerminalBusy(false);
          return;
        }
        const terminalId = String(chosen?.id || '').trim();
        if (!terminalId) {
          return;
        }
        setActiveTerminalBusy(Boolean(chosen?.busy));
        setActiveRun((prev) => {
          if (prev?.origin === 'dispatched' && prev.terminalId === terminalId) {
            return {
              ...prev,
              terminalName: String(chosen?.name || terminalId),
              cwd: String(chosen?.cwd || prev.cwd || runTargetCwd || projectRootPath || ''),
            };
          }
          return {
            terminalId,
            terminalName: String(chosen?.name || terminalId),
            command: prev?.command || currentCommand || selectedRunTargetCommand || '',
            cwd: String(chosen?.cwd || runTargetCwd || projectRootPath || ''),
            dispatchedAt: prev?.dispatchedAt || Date.now(),
            origin: 'discovered',
          };
        });
      } catch {
        // ignore refresh errors
      }
    };

    void refresh();
    return () => {
      disposed = true;
    };
  }, [
    currentCommand,
    onListTerminals,
    projectId,
    projectRootPath,
    runTargetCwd,
    selectedRunTargetCommand,
    setActiveRun,
    setActiveTerminalBusy,
  ]);

  useProjectRunRealtime({
    enabled: Boolean(projectId),
    projectId,
    onRunStateChanged: async (payload) => {
      const terminalId = String(payload?.terminal_id || '').trim();
      const payloadStatus = String(payload?.status || '').trim();
      const payloadCwd = String(payload?.cwd || '').trim();
      const payloadTerminalName = String(payload?.terminal_name || '').trim();

      setActiveTerminalBusy(Boolean(payload.busy));

      if (payloadStatus === 'exited') {
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
        terminalName: payloadTerminalName || prev?.terminalName || terminalId,
        command: prev?.command || currentCommand || selectedRunTargetCommand || '',
        cwd: payloadCwd || prev?.cwd || runTargetCwd || projectRootPath || '',
        dispatchedAt: prev?.dispatchedAt || Date.now(),
        origin: prev?.origin === 'dispatched' && prev.terminalId === terminalId ? prev.origin : 'discovered',
      }));
    },
  });

  useTerminalStateRealtime({
    enabled: Boolean(activeRunTerminalId),
    terminalId: activeRunTerminalId,
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
        const nextTerminalName = String(payload.terminal_name || '').trim() || prev.terminalName;
        const nextCwd = String(payload.cwd || '').trim() || prev.cwd;
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
};
