import { describe, expect, it } from 'vitest';

import type { ChatStoreDraft } from '../../lib/store/types';
import type { RealtimeChatStreamPayloadWrapper } from '../../lib/realtime/types';
import {
  asRealtimeParsedEvent,
  buildRealtimeCompletionKey,
  collectActiveStreamingSessionIds,
  resolveActiveStreamContext,
  resolveLatestStreamedText,
  resolvePersistedRealtimeStreamContext,
  resolvePayloadConversationTurnId,
  shouldAttemptDisconnectRecovery,
  shouldRecoverMessagesForActiveSession,
} from './chatStreamRealtimeBridgeState';

const buildState = (): ChatStoreDraft => ({
  currentSessionId: 'session_1',
  sessionChatState: {
    session_1: {
      isLoading: false,
      isStreaming: true,
      isStopping: false,
      streamingMessageId: 'assistant_temp_1',
      activeTurnId: 'turn_1',
      streamingPreviewText: '',
      streamingTransport: 'realtime',
      runtimeContextRefreshNonce: 0,
    },
  },
  sessionStreamingMessageDrafts: {
    session_1: {
      id: 'assistant_temp_1',
      sessionId: 'session_1',
      role: 'assistant',
      content: 'hello world',
      status: 'streaming',
      createdAt: new Date('2026-05-20T10:00:00.000Z'),
      metadata: {
        conversation_turn_id: 'turn_1',
        historyDraftUserMessage: {
          id: 'user_temp_1',
        },
      },
    },
  },
} as unknown as ChatStoreDraft);

describe('chatStreamRealtimeBridgeState', () => {
  it('collects active streaming session ids deterministically', () => {
    expect(collectActiveStreamingSessionIds({
      session_b: {
        isLoading: false,
        isStreaming: true,
        isStopping: false,
        streamingMessageId: 'msg_b',
        activeTurnId: null,
        streamingPreviewText: '',
      },
      session_a: {
        isLoading: false,
        isStreaming: true,
        isStopping: false,
        streamingMessageId: 'msg_a',
        activeTurnId: null,
        streamingPreviewText: '',
      },
      session_empty: {
        isLoading: false,
        isStreaming: false,
        isStopping: false,
        streamingMessageId: null,
        activeTurnId: null,
        streamingPreviewText: '',
      },
    } as ChatStoreDraft['sessionChatState'])).toEqual(['session_a', 'session_b']);
  });

  it('resolves active stream context and latest streamed text', () => {
    const state = buildState();
    expect(resolveActiveStreamContext(state, 'session_1')).toEqual({
      sessionId: 'session_1',
      conversationTurnId: 'turn_1',
      tempAssistantMessageId: 'assistant_temp_1',
      tempUserId: 'user_temp_1',
      streamedTextRef: { value: 'hello world' },
    });
    expect(resolveLatestStreamedText(state, 'session_1', 'fallback')).toBe('hello world');
  });

  it('reads turn ids and parsed event types from realtime payloads', () => {
    const payload = {
      kind: 'chat_stream',
      conversation_id: 'session_1',
      stream_type: 'message_delta',
      conversation_turn_id: 'turn_a',
      raw: {
        turn_id: 'turn_b',
      },
    } as RealtimeChatStreamPayloadWrapper;

    expect(resolvePayloadConversationTurnId(payload)).toBe('turn_a');
    expect(asRealtimeParsedEvent({
      kind: 'chat_stream',
      conversation_id: 'session_1',
      stream_type: 'message_delta',
      raw: {},
    } as RealtimeChatStreamPayloadWrapper)).toMatchObject({
      type: 'message_delta',
    });
  });

  it('builds stable completion keys and active-session recovery guards', () => {
    expect(buildRealtimeCompletionKey(
      'session_1',
      'assistant_1',
      'done',
      'complete',
      '2026-05-20T10:00:00.000Z',
    )).toBe('session_1:assistant_1:complete:2026-05-20T10:00:00.000Z');
    expect(buildRealtimeCompletionKey(
      'session_1',
      'assistant_1',
      'error',
      null,
      '2026-05-20T10:00:00.000Z',
    )).toBe('session_1:assistant_1:error:2026-05-20T10:00:00.000Z');

    const state = buildState();
    expect(shouldAttemptDisconnectRecovery(state, 'session_1', 'disconnected')).toBe(true);
    expect(shouldAttemptDisconnectRecovery(state, 'session_1', 'connected')).toBe(false);
    expect(shouldRecoverMessagesForActiveSession(state, 'session_1')).toBe(true);
    expect(shouldRecoverMessagesForActiveSession(state, 'session_2')).toBe(false);
  });

  it('reconstructs terminal stream context from persisted realtime messages when live draft state is gone', () => {
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
    } as unknown as ChatStoreDraft;

    expect(resolvePersistedRealtimeStreamContext(state, 'session_1', {
      payloadTurnId: 'turn_1',
      payloadUserMessageId: 'user_1',
      persistedUserMessage: {
        id: 'user_1',
        sessionId: 'session_1',
        role: 'user',
        content: 'hello',
        status: 'completed',
        createdAt: new Date('2026-05-20T10:00:00.000Z'),
        metadata: {
          conversation_turn_id: 'turn_1',
        },
      } as never,
      persistedAssistantMessage: {
        id: 'assistant_1',
        sessionId: 'session_1',
        role: 'assistant',
        content: 'final answer',
        status: 'completed',
        createdAt: new Date('2026-05-20T10:00:02.000Z'),
        metadata: {
          conversation_turn_id: 'turn_1',
          historyFinalForUserMessageId: 'user_1',
        },
      } as never,
    })).toEqual({
      sessionId: 'session_1',
      conversationTurnId: 'turn_1',
      tempAssistantMessageId: 'assistant_temp_1',
      tempUserId: 'user_temp_1',
      streamedTextRef: { value: 'draft answer' },
    });
  });
});
