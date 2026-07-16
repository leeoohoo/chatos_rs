// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

import type ApiClient from '../../../api/client';
import type { LocalRuntimeEventRecord } from '../../../api/localRuntime/types';
import type { ChatStoreSet, ChatStoreShape } from '../../types';
import {
  applyLocalRuntimeEvents,
  startLocalRuntimeEventPolling,
} from './localEvents';

const event = (
  eventSequence: number,
  eventName: string,
  payload: unknown,
): LocalRuntimeEventRecord => ({
  event_seq: eventSequence,
  event_id: `event_${eventSequence}`,
  session_id: 'lc_session_1',
  turn_id: 'lc_turn_1',
  event_name: eventName,
  payload,
  created_at: '2026-07-15T00:00:00Z',
});

const createState = (activeTurnId = 'lc_turn_1') => ({
  currentSessionId: 'lc_session_1',
  isLoading: true,
  isStreaming: false,
  sessionChatState: {
    lc_session_1: {
      isLoading: true,
      isStreaming: false,
      isStopping: false,
      streamingPhase: null,
      streamingMessageId: null,
      activeTurnId,
      streamingPreviewText: '',
      streamingTransport: 'local',
    },
  },
} as unknown as ChatStoreShape);

const setter = (state: ChatStoreShape): ChatStoreSet => (update) => update(state);

describe('applyLocalRuntimeEvents', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('applies out-of-order event batches by SQLite sequence', () => {
    const state = createState();

    applyLocalRuntimeEvents({
      set: setter(state),
      sessionId: 'lc_session_1',
      turnId: 'lc_turn_1',
    }, [
      event(3, 'chat.chunk', { text: ' world' }),
      event(1, 'chat.thinking', { text: 'reasoning' }),
      event(2, 'chat.chunk', { text: 'hello' }),
    ]);

    expect(state.sessionChatState.lc_session_1.streamingPreviewText).toBe('hello world');
    expect(state.sessionChatState.lc_session_1.streamingPhase).toBeNull();
    expect(state.sessionChatState.lc_session_1.isStreaming).toBe(true);
    expect(state.isLoading).toBe(false);
    expect(state.isStreaming).toBe(true);
  });

  it('ignores events belonging to a stale local turn', () => {
    const state = createState('lc_turn_new');

    applyLocalRuntimeEvents({
      set: setter(state),
      sessionId: 'lc_session_1',
      turnId: 'lc_turn_1',
    }, [event(1, 'chat.chunk', { text: 'stale' })]);

    expect(state.sessionChatState.lc_session_1.streamingPreviewText).toBe('');
    expect(state.sessionChatState.lc_session_1.isStreaming).toBe(false);
  });

  it('polls incrementally and performs a final drain when stopped', async () => {
    vi.useFakeTimers();
    const state = createState();
    const getRuntimeEvents = vi
      .fn()
      .mockResolvedValueOnce([event(1, 'chat.chunk', { text: 'live' })])
      .mockResolvedValue([]);
    const client = {
      getLocalRuntimeClient: () => ({ getRuntimeEvents }),
    } as unknown as ApiClient;
    const polling = startLocalRuntimeEventPolling({
      client,
      set: setter(state),
      sessionId: 'lc_session_1',
      turnId: 'lc_turn_1',
    });
    await vi.advanceTimersByTimeAsync(1);

    expect(state.sessionChatState.lc_session_1.streamingPreviewText).toBe('live');
    await polling.stop();
    expect(getRuntimeEvents).toHaveBeenCalledTimes(2);
    expect(getRuntimeEvents.mock.calls[1]?.[1]).toMatchObject({ after: 1 });
  });
});
