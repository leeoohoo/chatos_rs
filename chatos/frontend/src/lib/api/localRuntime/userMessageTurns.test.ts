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

  it('marks deferred local project tasks as awaiting confirmation instead of running', () => {
    const response = buildLocalUserMessageTurns(
      [
        {
          id: 'message-user',
          turn_id: 'turn-plan',
          sequence_no: 1,
          role: 'user',
          content: 'Plan local tasks',
          metadata: {
            project_requirement_execution: {
              project_id: 'project-1',
              requirement_id: 'requirement-1',
            },
            task_runner_async: {
              mode: 'project_requirement_execution',
              overall_status: 'planning',
            },
          },
          created_at: '2026-07-15T01:00:00Z',
        },
      ],
      [
        {
          id: 'task-plan',
          title: 'Deferred task',
          status: 'todo',
          last_run_id: null,
          source_turn_id: 'turn-plan',
          source_user_message_id: 'message-user',
        },
      ],
    );

    expect(response.items[0].user_message.metadata?.task_runner_async).toMatchObject({
      mode: 'project_requirement_execution',
      created_task_ids: ['task-plan'],
      running_task_ids: [],
      overall_status: 'awaiting_confirmation',
      confirmation_status: 'awaiting_confirmation',
    });
  });

  it('does not offer confirmation for a partially generated graph marked failed', () => {
    const response = buildLocalUserMessageTurns(
      [{
        id: 'message-user',
        turn_id: 'turn-failed',
        sequence_no: 1,
        role: 'user',
        content: 'Plan local tasks',
        metadata: {
          task_runner_async: {
            mode: 'project_requirement_execution',
            overall_status: 'failed',
            confirmation_status: 'failed',
          },
        },
        created_at: '2026-07-15T01:00:00Z',
      }],
      [{
        id: 'task-partial',
        title: 'Partial task',
        status: 'todo',
        last_run_id: null,
        source_turn_id: 'turn-failed',
        source_user_message_id: 'message-user',
      }],
    );

    expect(response.items[0].user_message.metadata?.task_runner_async).toMatchObject({
      overall_status: 'failed',
      confirmation_status: 'failed',
      running_task_ids: [],
    });
  });

  it('does not revive a completed user turn from local Task Manager checklist rows', () => {
    const response = buildLocalUserMessageTurns(
      [
        {
          id: 'message-user',
          turn_id: 'turn-parent',
          sequence_no: 1,
          role: 'user',
          content: 'Run browser smoke test',
          metadata: {
            task_runner_async: {
              overall_status: 'completed',
              created_task_ids: ['parent-task'],
            },
          },
          created_at: '2026-07-15T01:00:00Z',
        },
      ],
      [
        {
          id: 'parent-task',
          title: 'Parent task',
          status: 'done',
          task_kind: 'task_runner',
          source_turn_id: 'turn-parent',
          source_user_message_id: 'message-user',
        },
        {
          id: 'checklist-task',
          title: 'Stale checklist task',
          status: 'doing',
          task_kind: 'task_manager',
          source_turn_id: 'turn-parent',
          source_user_message_id: 'message-user',
        },
      ],
    );

    expect(response.items[0].user_message.metadata?.task_runner_async).toMatchObject({
      overall_status: 'completed',
      running_task_ids: [],
    });
  });
});
