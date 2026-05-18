import { useEffect } from 'react';

import type { TerminalLogResponse } from '../../../lib/api/client/types';
import type { ProjectRunnerActiveTerminal } from '../../../lib/domain/projectRunner';
import { extractFailureReasonFromLogs } from './projectRunnerFailureDiagnostics';

interface UseProjectRunnerExitInspectionOptions {
  lastExitedRun: ProjectRunnerActiveTerminal | null;
  lastExitCheckedRunKey: string;
  manualControlAt: number;
  onListTerminalLogs: (
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string },
  ) => Promise<TerminalLogResponse[]>;
  setLastExitCheckedRunKey: (value: string) => void;
  setRunnerError: (value: string | null) => void;
  setRunnerDiagnosis: (value: string | null) => void;
  setRunnerMessage: (value: string | null) => void;
}

export const useProjectRunnerExitInspection = ({
  lastExitedRun,
  lastExitCheckedRunKey,
  manualControlAt,
  onListTerminalLogs,
  setLastExitCheckedRunKey,
  setRunnerError,
  setRunnerDiagnosis,
  setRunnerMessage,
}: UseProjectRunnerExitInspectionOptions) => {
  useEffect(() => {
    if (!lastExitedRun?.terminalId) {
      return;
    }
    if (lastExitedRun.origin !== 'dispatched') {
      return;
    }

    const runKey = `${lastExitedRun.terminalId}:${lastExitedRun.dispatchedAt}`;
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
        if (lastExitedRun.exitReason === 'closed') {
          setRunnerDiagnosis('运行已停止');
          setRunnerError(null);
          return;
        }
        if (lastExitedRun.exitCode === 0) {
          setRunnerDiagnosis('进程已正常退出');
          setRunnerError(null);
          return;
        }
        const logs = await onListTerminalLogs(lastExitedRun.terminalId, { limit: 120, offset: 0 });
        if (disposed) {
          return;
        }
        const reason = extractFailureReasonFromLogs(logs || [], lastExitedRun.command);
        if (reason) {
          setRunnerDiagnosis(reason);
          setRunnerError(`运行失败：${reason}`);
          setRunnerMessage(null);
          return;
        }
        if (typeof lastExitedRun.exitCode === 'number') {
          setRunnerDiagnosis(`进程已退出，退出码 ${lastExitedRun.exitCode}`);
          setRunnerError(`运行失败：进程已退出，退出码 ${lastExitedRun.exitCode}`);
          setRunnerMessage(null);
          return;
        }
        setRunnerDiagnosis('进程已退出，但没有识别出明确的失败原因');
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
    lastExitedRun,
    lastExitCheckedRunKey,
    manualControlAt,
    onListTerminalLogs,
    setLastExitCheckedRunKey,
    setRunnerDiagnosis,
    setRunnerError,
    setRunnerMessage,
  ]);
};
