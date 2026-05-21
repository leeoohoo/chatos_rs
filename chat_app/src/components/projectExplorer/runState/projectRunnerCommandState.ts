import type { ProjectRunnerActiveTerminal } from '../../../lib/domain/projectRunner';
import type { ProjectRunTarget } from '../../../types';

const readTrimmedString = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');

export const buildProjectRunnerSelectedTerminalId = (value: unknown): string | null => {
  const nextValue = readTrimmedString(value);
  return nextValue || null;
};

export const buildProjectRunnerDispatchState = ({
  target,
  terminalId,
  terminalName,
  commandPreview,
}: {
  target: ProjectRunTarget;
  terminalId: string | null;
  terminalName: string | null;
  commandPreview: string;
}): ProjectRunnerActiveTerminal | null => {
  if (!terminalId) {
    return null;
  }
  return {
    terminalId,
    terminalName: terminalName || terminalId,
    cwd: target.cwd,
    command: commandPreview || target.command,
    dispatchedAt: Date.now(),
    origin: 'dispatched',
    exitCode: null,
    exitReason: null,
  };
};

export const resolveProjectRunnerDeleteTarget = (
  terminalIds: string[],
  terminalId: string,
): string | null => {
  const normalizedTerminalId = readTrimmedString(terminalId);
  if (!normalizedTerminalId) {
    return null;
  }
  const currentIndex = terminalIds.findIndex((item) => item === normalizedTerminalId);
  if (currentIndex < 0) {
    return null;
  }
  return terminalIds[currentIndex + 1]
    || terminalIds[currentIndex - 1]
    || null;
};
