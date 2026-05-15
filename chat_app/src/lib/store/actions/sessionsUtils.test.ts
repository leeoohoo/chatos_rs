import { describe, expect, it } from 'vitest';

import type { Message } from '../../../types';
import type { ChatStoreShape, SessionMessagesSnapshot } from '../types';
import {
  SESSION_MESSAGES_CACHE_MAX_ENTRIES,
  deleteSessionMessagesCacheEntry,
  readSessionMessagesCache,
  touchSessionMessagesCacheEntry,
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
});
