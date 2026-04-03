import type { Terminal, TerminalLog } from '../../../types';

export const normalizeTerminal = (raw: any): Terminal => ({
  id: raw?.id,
  name: raw?.name ?? '',
  cwd: raw?.cwd ?? '',
  userId: raw?.user_id ?? raw?.userId ?? null,
  projectId: raw?.project_id ?? raw?.projectId ?? null,
  status: raw?.status ?? 'running',
  busy: raw?.busy ?? false,
  createdAt: new Date(raw?.created_at ?? raw?.createdAt ?? Date.now()),
  updatedAt: new Date(raw?.updated_at ?? raw?.updatedAt ?? raw?.created_at ?? raw?.createdAt ?? Date.now()),
  lastActiveAt: new Date(raw?.last_active_at ?? raw?.lastActiveAt ?? raw?.updated_at ?? raw?.updatedAt ?? Date.now()),
});

const sameDate = (left?: Date | null, right?: Date | null): boolean => {
  const leftTime = left instanceof Date ? left.getTime() : 0;
  const rightTime = right instanceof Date ? right.getTime() : 0;
  return leftTime === rightTime;
};

export const areTerminalsEqual = (left: Terminal | null | undefined, right: Terminal | null | undefined): boolean => {
  if (!left && !right) {
    return true;
  }
  if (!left || !right) {
    return false;
  }
  return (
    left.id === right.id
    && left.name === right.name
    && left.cwd === right.cwd
    && (left.userId ?? null) === (right.userId ?? null)
    && (left.projectId ?? null) === (right.projectId ?? null)
    && left.status === right.status
    && Boolean(left.busy) === Boolean(right.busy)
    && sameDate(left.createdAt, right.createdAt)
    && sameDate(left.updatedAt, right.updatedAt)
    && sameDate(left.lastActiveAt, right.lastActiveAt)
  );
};

export const areTerminalListsEqual = (left: Terminal[], right: Terminal[]): boolean => {
  if (left === right) {
    return true;
  }
  if (left.length !== right.length) {
    return false;
  }
  for (let index = 0; index < left.length; index += 1) {
    if (!areTerminalsEqual(left[index], right[index])) {
      return false;
    }
  }
  return true;
};

export const normalizeTerminalLog = (raw: any): TerminalLog => ({
  id: raw?.id,
  terminalId: raw?.terminal_id ?? raw?.terminalId ?? '',
  logType: raw?.log_type ?? raw?.logType ?? raw?.type ?? '',
  content: raw?.content ?? '',
  createdAt: raw?.created_at ?? raw?.createdAt ?? '',
});
