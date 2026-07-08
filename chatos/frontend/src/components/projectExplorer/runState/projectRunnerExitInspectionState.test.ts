// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import { shouldInspectProjectRunnerExit } from './projectRunnerExitInspectionState';

describe('projectRunnerExitInspectionState', () => {
  it('marks manual-control exits as checked without inspecting', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-05-20T00:00:10Z'));

    expect(shouldInspectProjectRunnerExit({
      lastExitedRun: {
        terminalId: 'terminal_1',
        terminalName: 'Terminal 1',
        cwd: '/workspace',
        command: 'npm run dev',
        dispatchedAt: 123,
        origin: 'dispatched',
      },
      lastExitCheckedRunKey: '',
      manualControlAt: Date.parse('2026-05-20T00:00:08Z'),
    })).toEqual({
      shouldInspect: false,
      shouldMarkChecked: true,
      runKey: 'terminal_1:123',
    });

    vi.useRealTimers();
  });

  it('allows inspection for newly observed exits', () => {
    expect(shouldInspectProjectRunnerExit({
      lastExitedRun: {
        terminalId: 'terminal_1',
        terminalName: 'Terminal 1',
        cwd: '/workspace',
        command: 'npm run dev',
        dispatchedAt: 123,
        origin: 'dispatched',
      },
      lastExitCheckedRunKey: '',
      manualControlAt: 0,
    }).shouldInspect).toBe(true);
  });
});
