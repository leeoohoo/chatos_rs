// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { Message } from '../../../types';
import type { ChatStoreShape, SessionMessagesSnapshot } from '../types';
import {
  SESSION_MESSAGES_CACHE_MAX_ENTRIES,
  SESSION_MESSAGES_INITIAL_PAGE_SIZE,
  deleteSessionMessagesCacheEntry,
  mergeLatestCompactHistorySnapshot,
  readSessionMessagesCache,
  touchSessionMessagesCacheEntry,
  trimCompactHistorySnapshotToRecent,
  writeSessionMessagesCache,
} from './sessionsUtils';

const createMessage = (sessionId: string, id: string): Message => ({
  id,
  sessionId,
  role: 'assistant',
  content: id,
  status: 'completed',
  createdAt: new Date('2026-01-01T00:00:00.000Z'),
  metadata: {
    conversation_turn_id: `${id}_turn`,
  },
});

const createState = (): ChatStoreShape => ({
  sessionMessagesCache: {},
  sessionMessagesCacheOrder: [],
} as unknown as ChatStoreShape);

const writeSnapshot = (
  state: ChatStoreShape,
  sessionId: string,
  snapshot?: Partial<SessionMessagesSnapshot>,
) => {
  writeSessionMessagesCache(state, sessionId, {
    messages: snapshot?.messages ?? [createMessage(sessionId, `${sessionId}_message`)],
    nextBefore: snapshot?.nextBefore ?? `${sessionId}_before`,
    loaded: snapshot?.loaded ?? true,
  });
};

describe('sessionMessagesCache', () => {
  it('moves an existing session to the front when rewriting the snapshot', () => {
    const state = createState();

    writeSnapshot(state, 'session_1');
    writeSnapshot(state, 'session_2');
    writeSnapshot(state, 'session_3');
    writeSnapshot(state, 'session_1', {
      nextBefore: 'session_1_new_before',
    });

    expect(state.sessionMessagesCacheOrder).toEqual([
      'session_1',
      'session_3',
      'session_2',
    ]);
    expect(readSessionMessagesCache(state, 'session_1')).toMatchObject({
      nextBefore: 'session_1_new_before',
      loaded: true,
    });
  });

  it('moves an existing session to the front when touching a cached snapshot', () => {
    const state = createState();

    writeSnapshot(state, 'session_1');
    writeSnapshot(state, 'session_2');
    writeSnapshot(state, 'session_3');

    expect(touchSessionMessagesCacheEntry(state, 'session_1')).toBe(true);

    expect(state.sessionMessagesCacheOrder).toEqual([
      'session_1',
      'session_3',
      'session_2',
    ]);
    expect(touchSessionMessagesCacheEntry(state, 'missing_session')).toBe(false);
  });

  it('evicts the least recently used session after exceeding the cache limit', () => {
    const state = createState();

    for (let index = 1; index <= SESSION_MESSAGES_CACHE_MAX_ENTRIES; index += 1) {
      writeSnapshot(state, `session_${index}`);
    }
    writeSnapshot(state, 'session_1', {
      nextBefore: 'session_1_refresh',
    });
    writeSnapshot(state, `session_${SESSION_MESSAGES_CACHE_MAX_ENTRIES + 1}`);

    expect(state.sessionMessagesCacheOrder).toHaveLength(SESSION_MESSAGES_CACHE_MAX_ENTRIES);
    expect(state.sessionMessagesCacheOrder[0]).toBe(`session_${SESSION_MESSAGES_CACHE_MAX_ENTRIES + 1}`);
    expect(state.sessionMessagesCacheOrder).toContain('session_1');
    expect(state.sessionMessagesCacheOrder).not.toContain('session_2');
    expect(readSessionMessagesCache(state, 'session_2')).toBeNull();
    expect(readSessionMessagesCache(state, 'session_1')).toMatchObject({
      nextBefore: 'session_1_refresh',
      loaded: true,
    });
  });

  it('removes a deleted session from both cache storage and LRU order', () => {
    const state = createState();

    writeSnapshot(state, 'session_1');
    writeSnapshot(state, 'session_2');

    deleteSessionMessagesCacheEntry(state, 'session_1');

    expect(readSessionMessagesCache(state, 'session_1')).toBeNull();
    expect(state.sessionMessagesCacheOrder).toEqual(['session_2']);
    expect(state.sessionMessagesCache.session_1).toBeUndefined();
  });

  it('trims a cached snapshot to the most recent compact-history page', () => {
    const sessionId = 'session_1';
    const messages: Message[] = [];
    for (let index = 1; index <= SESSION_MESSAGES_INITIAL_PAGE_SIZE + 5; index += 1) {
      messages.push({
        id: `user_${index}`,
        sessionId,
        role: 'user',
        content: `user_${index}`,
        status: 'completed',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        metadata: {
          conversation_turn_id: `turn_${index}`,
        },
      });
      messages.push({
        id: `assistant_${index}`,
        sessionId,
        role: 'assistant',
        content: `assistant_${index}`,
        status: 'completed',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        metadata: {
          conversation_turn_id: `turn_${index}`,
          historyFinalForUserMessageId: `user_${index}`,
          historyFinalForTurnId: `turn_${index}`,
        },
      });
    }

    const trimmed = trimCompactHistorySnapshotToRecent({
      messages,
      nextBefore: 'turn_1',
      loaded: true,
    });

    expect(trimmed?.messages[0]?.id).toBe('user_6');
    expect(trimmed?.messages[trimmed.messages.length - 1]?.id).toBe(`assistant_${SESSION_MESSAGES_INITIAL_PAGE_SIZE + 5}`);
    expect(trimmed?.messages).toHaveLength(SESSION_MESSAGES_INITIAL_PAGE_SIZE * 2);
    expect(trimmed?.nextBefore).toBe('turn_6');
  });

  it('preserves offset cursors without trimming into a callback-only page', () => {
    const sessionId = 'session_1';
    const messages: Message[] = [];

    for (let index = 1; index <= SESSION_MESSAGES_INITIAL_PAGE_SIZE + 2; index += 1) {
      const turnId = `turn_${index}`;
      messages.push({
        id: `user_${index}`,
        sessionId,
        role: 'user',
        content: `user_${index}`,
        status: 'completed',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        metadata: {
          conversation_turn_id: turnId,
        },
      });
      messages.push({
        id: `assistant_${index}`,
        sessionId,
        role: 'assistant',
        content: `assistant_${index}`,
        status: 'completed',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        metadata: {
          conversation_turn_id: turnId,
          historyFinalForUserMessageId: `user_${index}`,
          historyFinalForTurnId: turnId,
        },
      });
    }

    messages.push({
      id: 'task_runner_callback::user_22::task_1::task.completed::run_1',
      sessionId,
      role: 'assistant',
      content: 'Task completed',
      status: 'completed',
      createdAt: new Date('2026-01-01T00:00:00.000Z'),
      metadata: {
        task_runner_async: {
          message_kind: 'task_terminal_update',
        },
      },
    });

    const trimmed = trimCompactHistorySnapshotToRecent({
      messages,
      nextBefore: 'offset:44',
      loaded: true,
    }, 1);

    expect(trimmed?.messages.map((message) => message.id)).toEqual([
      `user_${SESSION_MESSAGES_INITIAL_PAGE_SIZE + 2}`,
      `assistant_${SESSION_MESSAGES_INITIAL_PAGE_SIZE + 2}`,
      'task_runner_callback::user_22::task_1::task.completed::run_1',
    ]);
    expect(trimmed?.nextBefore).toBe('offset:44');
  });

  it('preserves an optimistic user turn when a background snapshot arrives before persistence', () => {
    const sessionId = 'session_1';
    const persisted = {
      ...createMessage(sessionId, 'assistant_old'),
      role: 'assistant' as const,
    };
    const optimistic: Message = {
      id: 'persisted_user_id_reserved_by_command',
      sessionId,
      role: 'user',
      content: 'new message',
      status: 'completed',
      createdAt: new Date('2026-01-01T00:01:00.000Z'),
      metadata: {
        clientOptimistic: true,
        conversation_turn_id: 'turn_new',
        task_runner_async: {
          mode: 'contact_async',
          overall_status: 'processing',
        },
      },
    };

    const merged = mergeLatestCompactHistorySnapshot(
      [persisted],
      null,
      {
        messages: [persisted, optimistic],
        nextBefore: null,
        loaded: true,
      },
    );

    expect(merged.messages.map((message) => message.id)).toEqual([
      'assistant_old',
      'persisted_user_id_reserved_by_command',
    ]);
  });

  it('replaces a temporary optimistic message when the persisted turn reaches the snapshot', () => {
    const sessionId = 'session_1';
    const optimistic: Message = {
      id: 'temp_user_1',
      sessionId,
      role: 'user',
      content: 'new message',
      status: 'completed',
      createdAt: new Date('2026-01-01T00:01:00.000Z'),
      metadata: {
        clientOptimistic: true,
        conversation_turn_id: 'turn_new',
      },
    };
    const persisted: Message = {
      ...optimistic,
      id: 'user_1',
      metadata: {
        conversation_turn_id: 'turn_new',
      },
    };

    const merged = mergeLatestCompactHistorySnapshot(
      [persisted],
      null,
      {
        messages: [optimistic],
        nextBefore: null,
        loaded: true,
      },
    );

    expect(merged.messages).toHaveLength(1);
    expect(merged.messages[0]?.id).toBe('user_1');
    expect(merged.messages[0]?.metadata?.clientOptimistic).not.toBe(true);
  });

});
