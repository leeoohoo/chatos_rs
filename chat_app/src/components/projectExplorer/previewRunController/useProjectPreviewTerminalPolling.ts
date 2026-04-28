import { useEffect } from 'react';

import { ApiRequestError } from '../../../lib/api/client/shared';
import type { TerminalResponse } from '../../../lib/api/client/types';
import type {
  ProjectPreviewRunSetter,
} from './previewRunControllerTypes';

interface UseProjectPreviewTerminalPollingOptions {
  activeRunTerminalId: string | null;
  currentCommand: string;
  projectId: string;
  projectRootPath: string;
  runTargetCwd: string;
  selectedRunTargetCommand: string | undefined;
  onGetTerminal: (terminalId: string) => Promise<TerminalResponse>;
  onListTerminals: () => Promise<TerminalResponse[]>;
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
  onGetTerminal,
  onListTerminals,
  setActiveRun,
  setActiveTerminalBusy,
}: UseProjectPreviewTerminalPollingOptions) => {
  useEffect(() => {
    if (!projectId) {
      return;
    }

    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const poll = async () => {
      try {
        const list = await onListTerminals();
        if (disposed || !Array.isArray(list)) {
          return;
        }
        const related = list
          .filter((item) => String(item?.project_id || item?.projectId || '') === projectId)
          .sort((a, b) => {
            const ta = new Date(a?.last_active_at || a?.lastActiveAt || 0).getTime();
            const tb = new Date(b?.last_active_at || b?.lastActiveAt || 0).getTime();
            return tb - ta;
          });
        const busy = related.find((item) => Boolean(item?.busy));
        const chosen = busy || related[0] || null;
        if (chosen) {
          const terminalId = String(chosen?.id || '').trim();
          if (terminalId) {
            setActiveTerminalBusy(Boolean(chosen?.busy));
            setActiveRun((prev) => {
              if (prev?.origin === 'dispatched' && prev.terminalId === terminalId) {
                return prev;
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
          }
        }
      } catch {
        // ignore discovery polling errors
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

  useEffect(() => {
    if (!activeRunTerminalId) {
      setActiveTerminalBusy(false);
      return;
    }

    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const poll = async () => {
      try {
        const terminal = await onGetTerminal(activeRunTerminalId);
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
  }, [activeRunTerminalId, onGetTerminal, setActiveTerminalBusy]);
};
