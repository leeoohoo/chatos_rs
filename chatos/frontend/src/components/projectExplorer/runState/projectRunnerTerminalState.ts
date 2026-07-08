// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { RealtimeTerminalStatePayloadWrapper } from '../../../lib/realtime/types';
import type { ProjectRunInstance, ProjectRunState } from '../../../types';
import type { ProjectRunnerActiveTerminal } from '../../../lib/domain/projectRunner';

const readTrimmedString = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');

export const buildProjectRunnerActiveRun = (
  instance: ProjectRunInstance | null | undefined,
  previous?: ProjectRunnerActiveTerminal | null,
): ProjectRunnerActiveTerminal | null => {
  if (!instance?.terminalId) {
    return null;
  }
  return {
    terminalId: instance.terminalId,
    terminalName: instance.terminalName || previous?.terminalName || instance.terminalId,
    command: previous?.command || '',
    cwd: instance.cwd || previous?.cwd || '',
    dispatchedAt: previous?.dispatchedAt || Date.now(),
    origin: previous?.origin || 'discovered',
    exitCode: previous?.exitCode ?? null,
    exitReason: previous?.exitReason ?? null,
  };
};

export const resolveProjectRunnerSelectedInstance = (
  instances: ProjectRunInstance[],
  selectedRunInstanceId: string | null,
): ProjectRunInstance | null => {
  if (!instances.length) {
    return null;
  }
  return instances.find((item) => item.terminalId === selectedRunInstanceId)
    || instances[0]
    || null;
};

export const removeProjectRunnerTerminalInstance = ({
  state,
  terminalId,
  nextSelectedTerminalId,
}: {
  state: ProjectRunState | null;
  terminalId: string;
  nextSelectedTerminalId?: string | null;
}): {
  nextState: ProjectRunState | null;
  nextSelectedRunInstanceId: string | null;
} => {
  const normalizedTerminalId = readTrimmedString(terminalId);
  const normalizedNextSelectedTerminalId = nextSelectedTerminalId === undefined
    ? undefined
    : readTrimmedString(nextSelectedTerminalId) || null;
  if (!normalizedTerminalId || !state) {
    return {
      nextState: state,
      nextSelectedRunInstanceId: normalizedNextSelectedTerminalId === undefined ? null : normalizedNextSelectedTerminalId,
    };
  }

  const nextInstances = (state.instances || []).filter((item) => item.terminalId !== normalizedTerminalId);
  const nextSelectedInstance = normalizedNextSelectedTerminalId === undefined
    ? (nextInstances[0] || null)
    : (normalizedNextSelectedTerminalId
      ? nextInstances.find((item) => item.terminalId === normalizedNextSelectedTerminalId) || null
      : null);

  return {
    nextState: {
      ...state,
      running: nextInstances.some((item) => item.running),
      busy: nextInstances.some((item) => item.busy),
      status: nextInstances.some((item) => item.running) ? 'running' : (nextInstances[0]?.status || 'idle'),
      terminalId: nextSelectedInstance?.terminalId || null,
      terminalName: nextSelectedInstance?.terminalName || null,
      cwd: nextSelectedInstance?.cwd || null,
      terminal: nextSelectedInstance?.terminal || null,
      instances: nextInstances,
    },
    nextSelectedRunInstanceId: normalizedNextSelectedTerminalId === undefined ? null : normalizedNextSelectedTerminalId,
  };
};

export const applyProjectRunnerTerminalStatePayload = ({
  state,
  selectedRunInstanceId,
  payload,
}: {
  state: ProjectRunState | null;
  selectedRunInstanceId: string | null;
  payload: RealtimeTerminalStatePayloadWrapper;
}): ProjectRunState | null => {
  if (!state) {
    return state;
  }

  const normalizedSelectedRunInstanceId = readTrimmedString(selectedRunInstanceId);
  const nextStatus = readTrimmedString(payload.status) || 'idle';
  const nextBusy = Boolean(payload.busy);
  const nextInstances = (state.instances || []).map((item) => {
    if (item.terminalId !== normalizedSelectedRunInstanceId) {
      return item;
    }
    return {
      ...item,
      status: nextStatus,
      busy: nextBusy,
      running: nextStatus === 'running',
      cwd: readTrimmedString(payload.cwd) || item.cwd,
      terminal: item.terminal
        ? {
          ...item.terminal,
          status: nextStatus,
          busy: nextBusy,
          cwd: readTrimmedString(payload.cwd) || item.terminal.cwd,
          name: readTrimmedString(payload.terminal_name) || item.terminal.name,
        }
        : item.terminal,
    };
  });
  const selected = resolveProjectRunnerSelectedInstance(nextInstances, normalizedSelectedRunInstanceId);

  return {
    ...state,
    running: nextInstances.some((item) => item.running),
    busy: nextInstances.some((item) => item.busy),
    status: nextInstances.some((item) => item.running) ? 'running' : (nextInstances[0]?.status || 'idle'),
    terminalId: selected?.terminalId || state.terminalId,
    terminalName: selected?.terminalName || state.terminalName,
    cwd: selected?.cwd || state.cwd,
    terminal: selected?.terminal || state.terminal,
    instances: nextInstances,
  };
};
