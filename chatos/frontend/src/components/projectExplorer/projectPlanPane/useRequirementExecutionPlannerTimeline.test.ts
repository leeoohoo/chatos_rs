// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { Message } from '../../../types';
import { isRequirementExecutionPlannerTimelineMessage } from './useRequirementExecutionPlannerTimeline';

const message = (role: Message['role'], metadata?: Message['metadata']): Message => ({
  id: `${role}-1`,
  sessionId: 'session-1',
  role,
  content: role === 'assistant' ? '最终模型输出' : '',
  status: 'completed',
  createdAt: new Date('2026-08-17T08:00:00Z'),
  metadata,
});

describe('requirement execution planner timeline selection', () => {
  it('includes final assistant output even when it is not marked as a process placeholder', () => {
    expect(isRequirementExecutionPlannerTimelineMessage(message('assistant'))).toBe(true);
  });

  it('includes loaded process records and excludes the planner user record', () => {
    expect(isRequirementExecutionPlannerTimelineMessage(message('tool', {
      historyProcessLoaded: true,
    }))).toBe(true);
    expect(isRequirementExecutionPlannerTimelineMessage(message('user'))).toBe(false);
  });
});
