import { describe, expect, it, vi } from 'vitest';

import { handleChatStreamRealtimeCompletion } from './chatStreamRealtimeCompletion';
import type { ChatStoreDraft } from '../../lib/store/types';

describe('chatStreamRealtimeCompletion', () => {
  it('ignores payloads without a conversation id', async () => {
    const apiClient = {};
    const chatStoreSet = vi.fn();

    await handleChatStreamRealtimeCompletion({
      payload: {
        kind: 'chat_stream',
        conversation_id: '',
        stream_type: 'message_delta',
        raw: {},
      } as never,
      storeGetState: () => ({}) as never,
      chatStoreSet: chatStoreSet as never,
      apiClient: apiClient as never,
      processedCompletionKeysRef: { current: new Set<string>() },
    });

    expect(chatStoreSet).not.toHaveBeenCalled();
  });

  it('reconciles persisted terminal messages even when streaming draft state was already cleared', async () => {
    const state = {
      currentSessionId: 'session_1',
      messages: [
        {
          id: 'user_temp_1',
          sessionId: 'session_1',
          role: 'user',
          content: 'hello',
          status: 'completed',
          createdAt: new Date('2026-05-20T10:00:00.000Z'),
          metadata: {
            conversation_turn_id: 'turn_1',
            historyProcess: {
              hasProcess: false,
              toolCallCount: 0,
              thinkingCount: 0,
              processMessageCount: 0,
              userMessageId: 'user_temp_1',
              turnId: 'turn_1',
              finalAssistantMessageId: null,
              loaded: true,
              loading: false,
            },
          },
        },
        {
          id: 'assistant_temp_1',
          sessionId: 'session_1',
          role: 'assistant',
          content: 'draft answer',
          status: 'streaming',
          createdAt: new Date('2026-05-20T10:00:01.000Z'),
          metadata: {
            conversation_turn_id: 'turn_1',
            historyDraftUserMessage: {
              id: 'user_temp_1',
            },
          },
        },
      ],
      sessionChatState: {
        session_1: {
          isLoading: false,
          isStreaming: false,
          isStopping: false,
          streamingMessageId: null,
          activeTurnId: null,
          streamingPreviewText: '',
          streamingTransport: null,
          runtimeContextRefreshNonce: 0,
        },
      },
      sessionStreamingMessageDrafts: {
        session_1: null,
      },
      syncSessionMessagesInBackground: vi.fn(async () => {}),
    } as unknown as ChatStoreDraft;
    const chatStoreSet = ((updater: (draft: ChatStoreDraft) => void) => updater(state)) as never;
    const apiClient = {
      getConversationTurnRuntimeContextByTurn: vi.fn(async () => ({
        conversation_id: 'session_1',
        turn_id: 'turn_1',
        status: 'completed',
        snapshot_source: 'runtime',
        snapshot: null,
      })),
      getConversationLatestTurnRuntimeContext: vi.fn(),
      getConversationTurnMessagesByTurn: vi.fn(async () => []),
      getConversationTurnMessages: vi.fn(async () => []),
    };

    await handleChatStreamRealtimeCompletion({
      payload: {
        kind: 'chat_stream',
        conversation_id: 'session_1',
        conversation_turn_id: 'turn_1',
        user_message_id: 'user_1',
        stream_type: 'complete',
        raw: {
          type: 'complete',
          timestamp: '2026-05-20T10:00:02.000Z',
          result: {
            persisted_user_message: {
              id: 'user_1',
              session_id: 'session_1',
              role: 'user',
              content: 'hello',
              status: 'completed',
              created_at: '2026-05-20T10:00:00.000Z',
              metadata: {
                conversation_turn_id: 'turn_1',
              },
            },
            persisted_assistant_message: {
              id: 'assistant_1',
              session_id: 'session_1',
              role: 'assistant',
              content: 'final answer',
              status: 'completed',
              created_at: '2026-05-20T10:00:02.000Z',
              metadata: {
                conversation_turn_id: 'turn_1',
                historyFinalForTurnId: 'turn_1',
                historyFinalForUserMessageId: 'user_1',
                contentSegments: [{ type: 'text', content: 'final answer' }],
              },
            },
          },
        },
      } as never,
      storeGetState: () => state,
      chatStoreSet,
      apiClient: apiClient as never,
      processedCompletionKeysRef: { current: new Set<string>() },
    });

    expect(state.messages.some((message) => message.id === 'assistant_temp_1')).toBe(false);
    expect(state.messages.find((message) => message.id === 'assistant_1')?.content).toBe('final answer');
    expect(state.messages.find((message) => message.id === 'assistant_1')?.status).toBe('completed');
    expect(state.sessionChatState.session_1.isStreaming).toBe(false);
  });
});
