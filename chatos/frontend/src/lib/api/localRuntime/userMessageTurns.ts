// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  MessageTaskRunnerTask,
  SessionMessageResponse,
  UserMessageTurnResponse,
  UserMessageTurnsResponse,
} from '../client/types';

export const buildLocalUserMessageTurns = (
  messages: SessionMessageResponse[],
  tasks: MessageTaskRunnerTask[],
  options: { limit?: number; before?: string | null } = {},
): UserMessageTurnsResponse => {
  const tasksByTurn = groupTasksByTurn(tasks);
  const messagesByTurn = new Map<string, SessionMessageResponse[]>();
  messages.forEach((message) => {
    const turnId = String(message.turn_id || '').trim();
    if (!turnId) return;
    const items = messagesByTurn.get(turnId) || [];
    items.push(message);
    messagesByTurn.set(turnId, items);
  });
  const before = String(options.before || '').trim();
  const all = Array.from(messagesByTurn.entries())
    .map(([turnId, items]) => turnResponse(turnId, items, tasksByTurn.get(turnId) || []))
    .filter((item): item is UserMessageTurnResponse => Boolean(item))
    .filter((item) => !before || String(item.user_message.created_at || '') < before)
    .sort((left, right) => String(left.user_message.created_at).localeCompare(String(right.user_message.created_at)));
  const limit = Math.max(1, Math.min(options.limit || 10, 100));
  const selected = all.slice(Math.max(0, all.length - limit));
  return {
    items: selected,
    has_more: all.length > selected.length,
    next_before: all.length > selected.length
      ? String(selected[0]?.user_message.created_at || '') || null
      : null,
  };
};

const turnResponse = (
  turnId: string,
  messages: SessionMessageResponse[],
  tasks: MessageTaskRunnerTask[],
): UserMessageTurnResponse | null => {
  const ordered = [...messages].sort((left, right) => (
    Number(left.sequence_no || 0) - Number(right.sequence_no || 0)
  ));
  const user = ordered.find((message) => message.role === 'user');
  if (!user) return null;
  const assistant = [...ordered].reverse().find((message) => message.role === 'assistant') || null;
  const userMessage = tasks.length > 0 ? withTaskMetadata(user, tasks) : user;
  return {
    turn_id: turnId,
    user_message: userMessage,
    final_assistant_message: assistant,
    has_process: ordered.length > 2,
    tool_call_count: ordered.reduce((count, message) => (
      count + (Array.isArray(message.tool_calls) ? message.tool_calls.length : 0)
    ), 0),
    thinking_count: ordered.filter((message) => Boolean(
      (message as SessionMessageResponse & { reasoning?: unknown }).reasoning,
    )).length,
    process_message_count: Math.max(ordered.length - 2, 0),
  };
};

const withTaskMetadata = (
  message: SessionMessageResponse,
  tasks: MessageTaskRunnerTask[],
): SessionMessageResponse => {
  const ids = (status: string) => tasks
    .filter((task) => String(task.status || '').toLowerCase() === status)
    .map((task) => task.id);
  const running = [...ids('todo'), ...ids('doing')];
  return {
    ...message,
    metadata: {
      ...(message.metadata || {}),
      task_runner_async: {
        source_user_message_id: message.id,
        source_turn_id: message.turn_id,
        created_task_ids: tasks.map((task) => task.id),
        running_task_ids: running,
        blocked_task_ids: ids('blocked'),
        succeeded_task_ids: ids('done'),
        overall_status: running.length > 0 ? 'running' : 'completed',
      },
    },
  };
};

const groupTasksByTurn = (
  tasks: MessageTaskRunnerTask[],
): Map<string, MessageTaskRunnerTask[]> => tasks.reduce((result, task) => {
  const turnId = String(task.source_turn_id || '').trim();
  if (!turnId) return result;
  const values = result.get(turnId) || [];
  values.push(task);
  result.set(turnId, values);
  return result;
}, new Map<string, MessageTaskRunnerTask[]>());
