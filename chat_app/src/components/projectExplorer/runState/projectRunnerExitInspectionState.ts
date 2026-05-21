import type { ProjectRunnerActiveTerminal } from '../../../lib/domain/projectRunner';

export const shouldInspectProjectRunnerExit = ({
  lastExitedRun,
  lastExitCheckedRunKey,
  manualControlAt,
}: {
  lastExitedRun: ProjectRunnerActiveTerminal | null;
  lastExitCheckedRunKey: string;
  manualControlAt: number;
}): {
  shouldInspect: boolean;
  shouldMarkChecked: boolean;
  runKey: string;
} => {
  if (!lastExitedRun?.terminalId || lastExitedRun.origin !== 'dispatched') {
    return { shouldInspect: false, shouldMarkChecked: false, runKey: '' };
  }

  const runKey = `${lastExitedRun.terminalId}:${lastExitedRun.dispatchedAt}`;
  if (runKey === lastExitCheckedRunKey) {
    return { shouldInspect: false, shouldMarkChecked: false, runKey };
  }
  if (manualControlAt > 0 && Date.now() - manualControlAt < 3500) {
    return { shouldInspect: false, shouldMarkChecked: true, runKey };
  }
  return { shouldInspect: true, shouldMarkChecked: false, runKey };
};
