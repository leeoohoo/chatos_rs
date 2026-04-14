import { describe, expect, it } from 'vitest';

import type { Message } from '../../../types';
import { mergeMessagesWithStreamingDraft } from './messagesState';

const createMessage = (id: string, content: string, status: Message['status'] = 'completed'): Message => ({
  id,
  sessionId: 'session_1',
  role: 'assistant',
  content,
  status,
  createdAt: new Date('2026-01-01T00:00:00.000Z'),
});

describe('mergeMessagesWithStreamingDraft', () => {
  it('does not append stale draft when session is no longer streaming', () => {
    const messages = [createMessage('final_assistant', 'final from server')];
    const staleDraft = createMessage('temp_assistant', 'stale local draft', 'completed');
    const state: any = {
      sessionChatState: {
        session_1: {
          isLoading: false,
          isStreaming: false,
          isStopping: false,
          streamingMessageId: null,
          activeTurnId: null,
        },
      },
      sessionStreamingMessageDrafts: {
        session_1: staleDraft,
      },
    };

    const merged = mergeMessagesWithStreamingDraft(state, 'session_1', messages);

    expect(merged).toEqual(messages);
    expect(merged.find((message) => message.id === 'temp_assistant')).toBeUndefined();
    expect(state.sessionStreamingMessageDrafts.session_1).toBeNull();
  });

  it('still injects draft while streaming if server list has no streaming message yet', () => {
    const messages = [createMessage('user_1', 'user', 'completed')];
    const draft = createMessage('temp_assistant', 'in-flight', 'streaming');
    const state: any = {
      sessionChatState: {
        session_1: {
          isLoading: true,
          isStreaming: true,
          isStopping: false,
          streamingMessageId: 'temp_assistant',
          activeTurnId: 'turn_1',
        },
      },
      sessionStreamingMessageDrafts: {
        session_1: draft,
      },
    };

    const merged = mergeMessagesWithStreamingDraft(state, 'session_1', messages);

    expect(merged.find((message) => message.id === 'temp_assistant')).toBeDefined();
  });
});

