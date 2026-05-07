import { useEffect } from 'react';

import type { TerminalLogResponse } from '../../../lib/api/client/types';
import type {
  ProjectPreviewActiveRunState,
  StringStateSetter,
} from './previewRunControllerTypes';
import { extractFailureReasonFromLogs } from './previewRunControllerUtils';

interface UseProjectPreviewExitInspectionOptions {
  activeRun: ProjectPreviewActiveRunState | null;
  activeTerminalBusy: boolean;
  lastExitCheckedRunKey: string;
  manualControlAt: number;
  onListTerminalLogs: (
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string },
  ) => Promise<TerminalLogResponse[]>;
  setLastExitCheckedRunKey: (value: string) => void;
  setRunError: StringStateSetter;
  setRunMessage: StringStateSetter;
}

export const useProjectPreviewExitInspection = ({
  activeRun,
  activeTerminalBusy,
  lastExitCheckedRunKey,
  manualControlAt,
  onListTerminalLogs,
  setLastExitCheckedRunKey,
  setRunError,
  setRunMessage,
}: UseProjectPreviewExitInspectionOptions) => {
  useEffect(() => {
    if (!activeRun?.terminalId) {
      return;
    }
    if (activeRun.origin !== 'dispatched') {
      return;
    }
    if (activeTerminalBusy) {
      return;
    }

    const runKey = `${activeRun.terminalId}:${activeRun.dispatchedAt}`;
    if (runKey === lastExitCheckedRunKey) {
      return;
    }
    if (manualControlAt > 0 && Date.now() - manualControlAt < 3500) {
      setLastExitCheckedRunKey(runKey);
      return;
    }

    let disposed = false;
    const inspect = async () => {
      try {
        const logs = await onListTerminalLogs(activeRun.terminalId, { limit: 80, offset: 0 });
        if (disposed) {
          return;
        }
        const reason = extractFailureReasonFromLogs(logs || [], activeRun.command);
        if (reason) {
          setRunError(`运行失败：${reason}`);
          setRunMessage(null);
        }
      } catch {
        // ignore log inspection errors
      } finally {
        if (!disposed) {
          setLastExitCheckedRunKey(runKey);
        }
      }
    };
    void inspect();
    return () => {
      disposed = true;
    };
  }, [
    activeRun,
    activeTerminalBusy,
    lastExitCheckedRunKey,
    manualControlAt,
    onListTerminalLogs,
    setLastExitCheckedRunKey,
    setRunError,
    setRunMessage,
  ]);
};
