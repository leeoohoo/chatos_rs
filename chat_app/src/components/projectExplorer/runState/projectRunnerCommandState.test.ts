import { describe, expect, it, vi } from 'vitest';

import type { ProjectRunTarget } from '../../../types';
import {
  buildProjectRunnerDispatchState,
  buildProjectRunnerSelectedTerminalId,
  resolveProjectRunnerDeleteTarget,
} from './projectRunnerCommandState';

const target: ProjectRunTarget = {
  id: 'target_1',
  label: 'Target 1',
  kind: 'node',
  cwd: '/workspace/project_1',
  command: 'npm run dev',
  source: 'analyzer',
  confidence: 1,
  requiredToolchains: [],
};

describe('projectRunnerCommandState', () => {
  it('normalizes selected terminal ids', () => {
    expect(buildProjectRunnerSelectedTerminalId('  terminal_1  ')).toBe('terminal_1');
    expect(buildProjectRunnerSelectedTerminalId('')).toBeNull();
  });

  it('builds dispatch state from the selected target', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-05-20T00:00:00Z'));

    expect(buildProjectRunnerDispatchState({
      target,
      terminalId: 'terminal_1',
      terminalName: '',
      commandPreview: '',
    })).toEqual({
      terminalId: 'terminal_1',
      terminalName: 'terminal_1',
      cwd: '/workspace/project_1',
      command: 'npm run dev',
      dispatchedAt: Date.parse('2026-05-20T00:00:00Z'),
      origin: 'dispatched',
      exitCode: null,
      exitReason: null,
    });

    vi.useRealTimers();
  });

  it('resolves the next terminal after deletion', () => {
    expect(resolveProjectRunnerDeleteTarget(['terminal_1', 'terminal_2', 'terminal_3'], 'terminal_2')).toBe('terminal_3');
    expect(resolveProjectRunnerDeleteTarget(['terminal_1', 'terminal_2'], 'missing')).toBeNull();
  });
});
