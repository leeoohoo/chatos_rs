import { describe, expect, it } from 'vitest';

import type { MessageTaskRunnerGraphResponse, MessageTaskRunnerTask } from '../../lib/api/client/types';
import { buildTaskSourceLookup } from './useMessageTaskGraph';

describe('buildTaskSourceLookup', () => {
  it('uses the clicked task source message instead of the current graph message', () => {
    const graph: MessageTaskRunnerGraphResponse = {
      root_task_ids: ['current'],
      source_session_id: 'session-1',
      source_turn_id: 'turn-current',
      source_user_message_id: 'user-current',
      nodes: [],
      edges: [],
    };
    const task: MessageTaskRunnerTask = {
      id: 'prereq-task',
      title: '前置任务',
      source_session_id: 'session-1',
      source_turn_id: 'turn-prereq',
      source_user_message_id: 'user-prereq',
    };

    expect(buildTaskSourceLookup({
      task,
      graph,
      fallbackMessageId: 'user-current',
      fallbackLookup: {
        sessionId: 'session-1',
        turnId: 'turn-current',
        sourceUserMessageId: 'user-current',
      },
    })).toEqual({
      messageId: 'user-prereq',
      lookup: {
        sessionId: 'session-1',
        turnId: 'turn-prereq',
        sourceUserMessageId: 'user-prereq',
      },
    });
  });

  it('falls back to session and turn lookup when a task has no source message id', () => {
    const graph: MessageTaskRunnerGraphResponse = {
      root_task_ids: ['current'],
      source_session_id: 'session-1',
      source_turn_id: 'turn-current',
      source_user_message_id: 'user-current',
      nodes: [],
      edges: [],
    };
    const task: MessageTaskRunnerTask = {
      id: 'turn-only-task',
      title: '轮次任务',
      source_session_id: 'session-1',
      source_turn_id: 'turn-prereq',
    };

    expect(buildTaskSourceLookup({
      task,
      graph,
      fallbackMessageId: 'user-current',
      fallbackLookup: {
        sessionId: 'session-1',
        turnId: 'turn-current',
        sourceUserMessageId: 'user-current',
      },
    })).toEqual({
      messageId: 'task-source-turn-only-task',
      lookup: {
        sessionId: 'session-1',
        turnId: 'turn-prereq',
        sourceUserMessageId: null,
      },
    });
  });
});
