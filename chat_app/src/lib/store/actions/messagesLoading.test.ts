import { describe, expect, it, vi } from 'vitest';

import type { Message } from '../../../types';
import type {
  ChatStoreDraft,
  ChatStoreShape,
} from '../types';
import { createMessageLoadingActions } from './messagesLoading';

vi.mock('../helpers/messages', () => ({
  fetchSessionMessages: vi.fn(),
  applyTurnProcessCache: vi.fn((messages: Message[]) => messages),
}));

import { fetchSessionMessages } from '../helpers/messages';

const createMessage = (
  id: string,
  content: string,
  status: Message['status'] = 'completed',
): Message => ({
  id,
  sessionId: 'session_2',
  role: 'assistant',
  content,
  status,
  createdAt: new Date('2026-01-01T00:00:00.000Z'),
});

describe('syncSessionMessagesInBackground', () => {
  it('does not clear global loading or error state while settling another session', async () => {
    const finalAssistant = {
      ...createMessage('assistant_final', 'final from server'),
      metadata: {
        conversation_turn_id: 'turn_2',
        historyFinalForTurnId: 'turn_2',
      },
    } as Message;
    const draft = {
      ...createMessage('assistant_temp', 'stale local draft', 'streaming'),
      metadata: {
        conversation_turn_id: 'turn_2',
      },
    } as Message;
    const state = {
      currentSessionId: 'session_1',
      messages: [],
      hasMoreMessages: true,
      isLoading: true,
      isStreaming: false,
      streamingMessageId: null,
      error: 'keep-existing-error',
      sessionChatState: {
        session_2: {
          isLoading: true,
          isStreaming: true,
          isStopping: false,
          streamingMessageId: 'assistant_temp',
          activeTurnId: 'turn_2',
          streamingPreviewText: 'stale local draft',
        },
      },
      sessionStreamingMessageDrafts: {
        session_2: draft,
      },
      sessionTurnProcessState: {},
      sessionTurnProcessCache: {},
    } as unknown as ChatStoreShape;
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    vi.mocked(fetchSessionMessages).mockResolvedValue([finalAssistant]);

    const actions = createMessageLoadingActions({
      set,
      get,
      client: {} as never,
    });

    await actions.syncSessionMessagesInBackground('session_2');

    expect(state.isLoading).toBe(true);
    expect(state.error).toBe('keep-existing-error');
    expect(state.messages).toEqual([]);
    expect(state.sessionStreamingMessageDrafts.session_2).toBeNull();
    expect(state.sessionChatState.session_2).toMatchObject({
      isLoading: false,
      isStreaming: false,
      isStopping: false,
      streamingMessageId: null,
      activeTurnId: null,
      streamingPreviewText: '',
    });
  });
});
