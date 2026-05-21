import { useEffect } from 'react';

import type { TerminalLogResponse } from '../../../lib/api/client/types';
import type { ProjectRunnerActiveTerminal } from '../../../lib/domain/projectRunner';
import { extractFailureReasonFromLogs } from './projectRunnerFailureReason';
import { shouldInspectProjectRunnerExit } from './projectRunnerExitInspectionState';

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
    const { shouldInspect, shouldMarkChecked, runKey } = shouldInspectProjectRunnerExit({
      lastExitedRun,
      lastExitCheckedRunKey,
      manualControlAt,
    });
    if (!runKey) {
      return;
    }
    if (shouldMarkChecked) {
      setLastExitCheckedRunKey(runKey);
      return;
    }
    if (!shouldInspect) {
      return;
    }
    const exitedRun = lastExitedRun as ProjectRunnerActiveTerminal;

    let disposed = false;
    const inspect = async () => {
      try {
        if (exitedRun.exitReason === 'closed') {
          setRunnerDiagnosis('运行已停止');
          setRunnerError(null);
          return;
        }
        if (exitedRun.exitCode === 0) {
          setRunnerDiagnosis('进程已正常退出');
          setRunnerError(null);
          return;
        }
        const logs = await onListTerminalLogs(exitedRun.terminalId, { limit: 120, offset: 0 });
        if (disposed) {
          return;
        }
        const reason = extractFailureReasonFromLogs(logs || [], exitedRun.command);
        if (reason) {
          setRunnerDiagnosis(reason);
          setRunnerError(`运行失败：${reason}`);
          setRunnerMessage(null);
          return;
        }
        if (typeof exitedRun.exitCode === 'number') {
          setRunnerDiagnosis(`进程已退出，退出码 ${exitedRun.exitCode}`);
          setRunnerError(`运行失败：进程已退出，退出码 ${exitedRun.exitCode}`);
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
