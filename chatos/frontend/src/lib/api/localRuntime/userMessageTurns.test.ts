// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { buildLocalUserMessageTurns } from './userMessageTurns';

describe('buildLocalUserMessageTurns', () => {
  it('attaches local task state to the existing user-message task drawer entry', () => {
    const response = buildLocalUserMessageTurns(
      [
        {
          id: 'message-user',
          turn_id: 'turn-1',
          sequence_no: 1,
          role: 'user',
          content: 'Implement local tasks',
          created_at: '2026-07-15T01:00:00Z',
        },
        {
          id: 'message-assistant',
          turn_id: 'turn-1',
          sequence_no: 2,
          role: 'assistant',
          content: 'Working',
          created_at: '2026-07-15T01:00:01Z',
        },
      ],
      [
        {
          id: 'task-1',
          title: 'Persist task',
          status: 'doing',
          source_turn_id: 'turn-1',
          source_user_message_id: 'message-user',
        },
      ],
    );

    expect(response.items).toHaveLength(1);
    expect(response.items[0].user_message.metadata?.task_runner_async).toMatchObject({
      source_user_message_id: 'message-user',
      running_task_ids: ['task-1'],
      overall_status: 'running',
    });
  });
});
