// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import type { Message } from '../../../types';
import type { ChatStoreDraft, ChatStoreShape } from '../types';
import { createMessageLoadingActions } from './messagesLoading';
import { SESSION_MESSAGES_INITIAL_PAGE_SIZE } from './sessionsUtils';

vi.mock('../helpers/messages', () => ({
  fetchSessionMessages: vi.fn(),
}));

import { fetchSessionMessages } from '../helpers/messages';

const createMessage = (id: string, createdAt: string): Message => ({
  id,
  sessionId: 'session_2',
  role: 'user',
  content: id,
  status: 'completed',
  createdAt: new Date(createdAt),
  metadata: { conversation_turn_id: `turn_${id}` },
});

const createState = (): ChatStoreShape => ({
  currentSessionId: 'session_2',
  messages: [],
  hasMoreMessages: false,
  isLoading: false,
  isStreaming: false,
  streamingMessageId: null,
  error: null,
  sessionChatState: {},
  sessionMessagePaginationState: {},
} as unknown as ChatStoreShape);

const createActions = (state: ChatStoreShape) => {
  const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
    updater(state as unknown as ChatStoreDraft);
  });
  return createMessageLoadingActions({
    set,
    get: () => state,
    client: {} as never,
  });
};

describe('message loading without a frontend message cache', () => {
  it('replaces the active message list with the exact server order', async () => {
    const state = createState();
    state.messages = [
      createMessage('new_message', '2026-08-21T08:10:00.000Z'),
      createMessage('stale_local_message', '2026-08-20T08:10:00.000Z'),
    ];
    const serverMessages = [
      createMessage('old_requirement_1', '2026-08-19T02:39:00.000Z'),
      createMessage('old_requirement_2', '2026-08-20T05:01:00.000Z'),
      createMessage('new_message', '2026-08-21T08:10:00.000Z'),
    ];
    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: serverMessages,
      hasMore: false,
      nextBefore: null,
    });

    await createActions(state).syncSessionMessagesInBackground('session_2');

    expect(state.messages.map((message) => message.id)).toEqual([
      'old_requirement_1',
      'old_requirement_2',
      'new_message',
    ]);
  });

  it('does not fetch or retain messages for a non-active session', async () => {
    const state = createState();
    state.currentSessionId = 'session_1';
    state.messages = [createMessage('active_session_message', '2026-08-21T08:10:00.000Z')];
    vi.mocked(fetchSessionMessages).mockClear();

    await createActions(state).syncSessionMessagesInBackground('session_2');

    expect(fetchSessionMessages).not.toHaveBeenCalled();
    expect(state.messages.map((message) => message.id)).toEqual(['active_session_message']);
  });

  it('prepends an older server page without caching it separately', async () => {
    const state = createState();
    state.messages = [createMessage('newest', '2026-08-21T08:10:00.000Z')];
    state.hasMoreMessages = true;
    state.sessionMessagePaginationState.session_2 = {
      nextBefore: 'turn_older',
      loaded: true,
    };
    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [createMessage('older', '2026-08-20T08:10:00.000Z')],
      hasMore: false,
      nextBefore: null,
    });

    await createActions(state).loadMoreMessages('session_2');

    expect(state.messages.map((message) => message.id)).toEqual(['older', 'newest']);
    expect(state.hasMoreMessages).toBe(false);
  });

  it('uses the configured compact-history page size for direct loads', async () => {
    const state = createState();
    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [],
      hasMore: false,
      nextBefore: null,
    });

    await createActions(state).loadMessages('session_2');

    expect(fetchSessionMessages).toHaveBeenCalledWith(
      {} as never,
      'session_2',
      { limit: SESSION_MESSAGES_INITIAL_PAGE_SIZE, before: null },
    );
  });
});
