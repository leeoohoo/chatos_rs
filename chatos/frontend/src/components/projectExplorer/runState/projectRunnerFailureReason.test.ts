// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { extractFailureReasonFromLogs } from './projectRunnerFailureReason';

describe('projectRunnerFailureReason', () => {
  it('extracts a direct failure reason from logs when the signature matches', () => {
    expect(extractFailureReasonFromLogs([
      { content: 'Error: could not find or load main class App' },
    ] as never, 'mvn spring-boot:run')).toBe('Error: could not find or load main class App');
  });

  it('falls back to a long-running command exit reason when logs are inconclusive', () => {
    expect(extractFailureReasonFromLogs([
      { content: 'something went wrong' },
    ] as never, 'npm run dev', (key) => (
      key === 'runSettings.failure.longRunningExited'
        ? '命令已退出，未检测到持续运行进程'
        : key
    ))).toBe('命令已退出，未检测到持续运行进程');
  });
});
