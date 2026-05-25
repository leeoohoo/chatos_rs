import { describe, expect, it } from 'vitest';

import type { Message } from '../../../types';
import type { ChatStoreShape } from '../types';
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
    const state = {
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
    } as unknown as ChatStoreShape;

    const merged = mergeMessagesWithStreamingDraft(state, 'session_1', messages);

    expect(merged).toEqual(messages);
    expect(merged.find((message) => message.id === 'temp_assistant')).toBeUndefined();
    expect(state.sessionStreamingMessageDrafts.session_1).toBeNull();
  });

  it('still injects draft while streaming if server list has no streaming message yet', () => {
    const messages = [createMessage('user_1', 'user', 'completed')];
    const draft = createMessage('temp_assistant', 'in-flight', 'streaming');
    const state = {
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
    } as unknown as ChatStoreShape;

    const merged = mergeMessagesWithStreamingDraft(state, 'session_1', messages);

    expect(merged.find((message) => message.id === 'temp_assistant')).toBeDefined();
  });

  it('clears local streaming state when server snapshot already has final assistant for the same turn', () => {
    const finalAssistant = {
      ...createMessage('assistant_final', 'final from server', 'completed'),
      metadata: {
        conversation_turn_id: 'turn_1',
        historyFinalForTurnId: 'turn_1',
      },
    } as Message;
    const draft = {
      ...createMessage('temp_assistant', 'stale in-flight', 'streaming'),
      metadata: {
        conversation_turn_id: 'turn_1',
      },
    } as Message;
    const state = {
      currentSessionId: 'session_1',
      isLoading: true,
      isStreaming: true,
      streamingMessageId: 'temp_assistant',
      sessionChatState: {
        session_1: {
          isLoading: true,
          isStreaming: true,
          isStopping: false,
          streamingMessageId: 'temp_assistant',
          activeTurnId: 'turn_1',
          streamingPreviewText: 'stale in-flight',
        },
      },
      sessionStreamingMessageDrafts: {
        session_1: draft,
      },
    } as unknown as ChatStoreShape;

    const merged = mergeMessagesWithStreamingDraft(state, 'session_1', [finalAssistant]);

    expect(merged).toEqual([finalAssistant]);
    expect(state.sessionStreamingMessageDrafts.session_1).toBeNull();
    expect(state.sessionChatState.session_1.isStreaming).toBe(false);
    expect(state.sessionChatState.session_1.streamingMessageId).toBeNull();
    expect(state.isStreaming).toBe(false);
    expect(state.streamingMessageId).toBeNull();
  });

  it('keeps streaming state during reviewing phase even if server has a completed assistant for same turn', () => {
    const finalAssistant = {
      ...createMessage('assistant_final', 'summary from server', 'completed'),
      metadata: {
        conversation_turn_id: 'turn_1',
        historyFinalForTurnId: 'turn_1',
      },
    } as Message;
    const draft = {
      ...createMessage('temp_assistant', 'review in progress', 'streaming'),
      metadata: {
        conversation_turn_id: 'turn_1',
      },
    } as Message;
    const state = {
      currentSessionId: 'session_1',
      isLoading: true,
      isStreaming: true,
      streamingMessageId: 'temp_assistant',
      sessionChatState: {
        session_1: {
          isLoading: true,
          isStreaming: true,
          isStopping: false,
          streamingPhase: 'reviewing',
          streamingMessageId: 'temp_assistant',
          activeTurnId: 'turn_1',
          streamingPreviewText: 'review in progress',
        },
      },
      sessionStreamingMessageDrafts: {
        session_1: draft,
      },
    } as unknown as ChatStoreShape;

    const merged = mergeMessagesWithStreamingDraft(state, 'session_1', [finalAssistant]);

    expect(merged.find((message) => message.id === 'temp_assistant')).toBeDefined();
    expect(state.sessionChatState.session_1.isStreaming).toBe(true);
    expect(state.sessionChatState.session_1.streamingPhase).toBe('reviewing');
    expect(state.isStreaming).toBe(true);
  });
});
