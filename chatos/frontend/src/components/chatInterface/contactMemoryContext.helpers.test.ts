// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { normalizeAgentRecalls } from './contactMemoryContext.helpers';

describe('normalizeAgentRecalls', () => {
  it('keeps multiple local project and agent recalls when requested', () => {
    const recalls = normalizeAgentRecalls([
      {
        id: 'recall-project',
        recall_key: 'session:one',
        recall_text: 'project memory',
        subject_type: 'project',
        level: 0,
        updated_at: '2026-07-15T00:00:00Z',
      },
      {
        id: 'recall-agent',
        recall_key: 'session:two',
        recall_text: 'agent memory',
        subject_type: 'agent',
        level: 1,
        updated_at: '2026-07-15T00:00:01Z',
      },
    ], 8);

    expect(recalls).toHaveLength(2);
    expect(recalls[0].subjectType).toBe('agent');
    expect(recalls[1].subjectType).toBe('project');
  });
});
