// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import {
  isActiveRequirementExecutionConflict,
  isTerminalRequirementExecutionStatus,
} from './ProjectPlanPane';

describe('ProjectPlanPane active execution recovery', () => {
  it('recognizes active planning and Task Runner conflicts that require a stop action', () => {
    expect(isActiveRequirementExecutionConflict(
      '项目任务正在执行或待执行，请先停止当前执行：T01',
    )).toBe(true);
    expect(isActiveRequirementExecutionConflict(
      '该需求已有正在生成或等待确认的执行计划，请先完成或停止当前计划',
    )).toBe(true);
    expect(isActiveRequirementExecutionConflict(
      'This local requirement execution scope already has active task runs',
    )).toBe(true);
    expect(isActiveRequirementExecutionConflict('模型配置不可用')).toBe(false);
  });

  it('refreshes Plan data when a requirement execution reaches a terminal state', () => {
    expect(isTerminalRequirementExecutionStatus('completed')).toBe(true);
    expect(isTerminalRequirementExecutionStatus('succeeded')).toBe(true);
    expect(isTerminalRequirementExecutionStatus('failed')).toBe(true);
    expect(isTerminalRequirementExecutionStatus('stopped')).toBe(true);
    expect(isTerminalRequirementExecutionStatus('paused')).toBe(false);
    expect(isTerminalRequirementExecutionStatus('running')).toBe(false);
  });
});
