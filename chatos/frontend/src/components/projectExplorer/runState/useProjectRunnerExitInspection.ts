// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
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
  const { t } = useI18n();

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
          setRunnerDiagnosis(t('runSettings.exit.stopped'));
          setRunnerError(null);
          return;
        }
        if (exitedRun.exitCode === 0) {
          setRunnerDiagnosis(t('runSettings.exit.normal'));
          setRunnerError(null);
          return;
        }
        const logs = await onListTerminalLogs(exitedRun.terminalId, { limit: 120, offset: 0 });
        if (disposed) {
          return;
        }
        const reason = extractFailureReasonFromLogs(logs || [], exitedRun.command, t);
        if (reason) {
          setRunnerDiagnosis(reason);
          setRunnerError(t('runSettings.exit.failed', { reason }));
          setRunnerMessage(null);
          return;
        }
        if (typeof exitedRun.exitCode === 'number') {
          const codeDiagnosis = t('runSettings.exit.code', { code: exitedRun.exitCode });
          setRunnerDiagnosis(codeDiagnosis);
          setRunnerError(t('runSettings.exit.failed', { reason: codeDiagnosis }));
          setRunnerMessage(null);
          return;
        }
        setRunnerDiagnosis(t('runSettings.exit.unknown'));
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
    t,
  ]);
};
