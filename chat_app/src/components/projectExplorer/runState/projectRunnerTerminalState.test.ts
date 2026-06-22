import { describe, expect, it } from 'vitest';

import type { ProjectRunState } from '../../../types';
import {
  applyProjectRunnerTerminalStatePayload,
  buildProjectRunnerActiveRun,
  removeProjectRunnerTerminalInstance,
  resolveProjectRunnerSelectedInstance,
} from './projectRunnerTerminalState';

const baseState: ProjectRunState = {
  projectId: 'project_1',
  running: true,
  busy: false,
  status: 'running',
  terminalId: 'terminal_1',
  terminalName: 'Terminal 1',
  cwd: '/workspace/project_1',
  terminal: {
    id: 'terminal_1',
    name: 'Terminal 1',
    cwd: '/workspace/project_1',
    status: 'running',
    busy: false,
    createdAt: new Date('2026-05-20T00:00:00Z'),
    updatedAt: new Date('2026-05-20T00:00:00Z'),
    lastActiveAt: new Date('2026-05-20T00:00:00Z'),
  },
  instances: [
    {
      terminalId: 'terminal_1',
      terminalName: 'Terminal 1',
      cwd: '/workspace/project_1',
      status: 'running',
      busy: false,
      running: true,
      terminal: {
        id: 'terminal_1',
        name: 'Terminal 1',
        cwd: '/workspace/project_1',
        status: 'running',
        busy: false,
        createdAt: new Date('2026-05-20T00:00:00Z'),
        updatedAt: new Date('2026-05-20T00:00:00Z'),
        lastActiveAt: new Date('2026-05-20T00:00:00Z'),
      },
    },
    {
      terminalId: 'terminal_2',
      terminalName: 'Terminal 2',
      cwd: '/workspace/project_1',
      status: 'idle',
      busy: false,
      running: false,
      terminal: null,
    },
  ],
};

describe('projectRunnerTerminalState', () => {
  it('resolves the active instance with fallback to the first instance', () => {
    expect(resolveProjectRunnerSelectedInstance(baseState.instances || [], 'terminal_2')?.terminalId).toBe('terminal_2');
    expect(resolveProjectRunnerSelectedInstance(baseState.instances || [], 'missing')?.terminalId).toBe('terminal_1');
  });

  it('builds active run metadata while preserving previous runtime fields', () => {
    expect(buildProjectRunnerActiveRun(baseState.instances?.[0], {
      terminalId: 'terminal_1',
      terminalName: 'Terminal 1',
      cwd: '/workspace/project_1',
      command: 'npm run dev',
      dispatchedAt: 123,
      origin: 'dispatched',
      exitCode: 1,
      exitReason: 'boom',
    })).toEqual({
      terminalId: 'terminal_1',
      terminalName: 'Terminal 1',
      cwd: '/workspace/project_1',
      command: 'npm run dev',
      dispatchedAt: 123,
      origin: 'dispatched',
      exitCode: 1,
      exitReason: 'boom',
    });
  });

  it('removes a terminal instance and re-resolves selection', () => {
    const result = removeProjectRunnerTerminalInstance({
      state: baseState,
      terminalId: 'terminal_1',
    });

    expect(result.nextState?.instances).toHaveLength(1);
    expect(result.nextState?.terminalId).toBe('terminal_2');
    expect(result.nextSelectedRunInstanceId).toBeNull();
  });

  it('applies terminal state payloads only to the selected instance', () => {
    const nextState = applyProjectRunnerTerminalStatePayload({
      state: baseState,
      selectedRunInstanceId: 'terminal_1',
      payload: {
        kind: 'terminal_state',
        terminal_id: 'terminal_1',
        status: 'exited',
        busy: false,
        reason: 'closed',
        exit_code: 0,
      },
    });

    expect(nextState?.status).toBe('exited');
    expect(nextState?.terminalId).toBe('terminal_1');
    expect(nextState?.instances?.[0].status).toBe('exited');
  });
});
