import { describe, expect, it, vi } from 'vitest';

import type { TurnRuntimeSnapshotLookupResponse } from '../../lib/api/client/types';
import type { Message } from '../../types';
import type { ChatStoreDraft } from '../../lib/store/types';
import {
  applyRealtimeTerminalFailure,
  applyRealtimeTerminalMessages,
  recoverMessagesAfterRealtimeTerminalEvent,
  settleRealtimeTerminalEvent,
  shouldReloadAfterRealtimeTerminalEvent,
} from './chatStreamRealtimeTerminalState';
import {
  resolvePersistedTurnMessages,
  shouldFinalizeRealtimeTerminalEvent,
} from './chatStreamRealtimeBridgeState';

const buildUser = (): Message => ({
  id: 'user_temp_1',
  sessionId: 'session_1',
  role: 'user',
  content: 'hello',
  status: 'completed',
  createdAt: new Date('2026-05-20T10:00:00.000Z'),
  metadata: {
    conversation_turn_id: 'turn_1',
  },
});

const buildAssistant = (overrides: Partial<Message> = {}): Message => ({
  id: 'assistant_temp_1',
  sessionId: 'session_1',
  role: 'assistant',
  content: 'streaming text',
  status: 'streaming',
  createdAt: new Date('2026-05-20T10:00:01.000Z'),
  metadata: {
    conversation_turn_id: 'turn_1',
    historyFinalForUserMessageId: 'user_temp_1',
    contentSegments: [{ type: 'text', content: 'streaming text' }],
  },
  ...overrides,
});

const buildState = (): ChatStoreDraft => ({
  currentSessionId: 'session_1',
  messages: [buildUser(), buildAssistant()],
  isLoading: true,
  isStreaming: true,
  streamingMessageId: 'assistant_temp_1',
  sessionChatState: {
    session_1: {
      isLoading: true,
      isStreaming: true,
      isStopping: false,
      streamingMessageId: 'assistant_temp_1',
      activeTurnId: 'turn_1',
      streamingPreviewText: 'streaming text',
      streamingTransport: 'realtime',
      runtimeContextRefreshNonce: 0,
    },
  },
  sessionStreamingMessageDrafts: {
    session_1: buildAssistant(),
  },
  loadMessages: vi.fn(async () => {}),
} as unknown as ChatStoreDraft);

describe('chatStreamRealtimeTerminalState', () => {
  it('dedupes terminal completion keys and resolves persisted messages', () => {
    const processed = new Set<string>();
    expect(shouldFinalizeRealtimeTerminalEvent(processed, 'key_1')).toBe(true);
    expect(shouldFinalizeRealtimeTerminalEvent(processed, 'key_1')).toBe(false);

    const persisted = resolvePersistedTurnMessages({
      result: {
        persisted_user_message: {
          id: 'user_1',
          session_id: 'session_1',
          role: 'user',
          content: 'hello',
          status: 'completed',
          created_at: '2026-05-20T10:00:00.000Z',
        },
        persisted_assistant_message: {
          id: 'assistant_1',
          session_id: 'session_1',
          role: 'assistant',
          content: 'done',
          status: 'completed',
          created_at: '2026-05-20T10:00:01.000Z',
        },
      },
    } as never, 'session_1');
    expect(persisted.persistedUserMessage?.id).toBe('user_1');
    expect(persisted.persistedAssistantMessage?.id).toBe('assistant_1');
  });

  it('applies terminal completion state and finalizes streaming session', () => {
    const state = buildState();
    const set = (updater: (draft: ChatStoreDraft) => void) => updater(state);

    applyRealtimeTerminalMessages(set, {
      sessionId: 'session_1',
      turnId: 'turn_1',
      tempAssistantMessageId: 'assistant_temp_1',
      tempUserId: 'user_temp_1',
    }, {
      persistedUserMessage: null,
      persistedAssistantMessage: null,
    });

    expect(state.sessionChatState.session_1.isStreaming).toBe(false);
    expect(state.sessionStreamingMessageDrafts.session_1).toBeNull();
  });

  it('applies terminal failure state and marks assistant as error', () => {
    const state = buildState();
    const set = (updater: (draft: ChatStoreDraft) => void) => updater(state);

    applyRealtimeTerminalFailure(
      set,
      {
        sessionId: 'session_1',
        turnId: 'turn_1',
        tempAssistantMessageId: 'assistant_temp_1',
        tempUserId: 'user_temp_1',
      },
      {
        persistedUserMessage: null,
        persistedAssistantMessage: null,
      },
      buildAssistant(),
      'request failed',
      'network error',
    );

    expect(state.messages.find((message) => message.id === 'assistant_temp_1')?.status).toBe('error');
  });

  it('checks whether terminal event should trigger message reload', () => {
    const state = buildState();
    expect(shouldReloadAfterRealtimeTerminalEvent(state, {
      sessionId: 'session_1',
      tempAssistantMessageId: 'assistant_temp_1',
      tempUserId: 'user_temp_1',
    })).toBe(true);
  });

  it('settles terminal success without reloading when local terminal state is already sufficient', async () => {
    const state = buildState();
    const set = (updater: (draft: ChatStoreDraft) => void) => updater(state);
    const apiClient = {
      getConversationTurnRuntimeContextByTurn: vi.fn(),
      getConversationLatestTurnRuntimeContext: vi.fn(),
      getConversationTurnMessagesByTurn: vi.fn(async () => []),
      getConversationTurnMessages: vi.fn(async () => []),
    };

    await settleRealtimeTerminalEvent(
      apiClient,
      set,
      () => state,
      {
        sessionId: 'session_1',
        turnId: 'turn_1',
        tempAssistantMessageId: 'assistant_temp_1',
        tempUserId: 'user_temp_1',
      },
      {
        persistedUserMessage: null,
        persistedAssistantMessage: null,
      },
      { kind: 'success' },
    );

    expect(state.loadMessages).not.toHaveBeenCalled();
    expect(state.sessionChatState.session_1.isStreaming).toBe(false);
  });

  it('falls back to loadMessages when snapshot recovery cannot rebuild the terminal turn', async () => {
    const state = {
      ...buildState(),
      messages: [],
    } as ChatStoreDraft;
    const set = (updater: (draft: ChatStoreDraft) => void) => updater(state);
    const terminalSnapshot: TurnRuntimeSnapshotLookupResponse = {
      conversation_id: 'session_1',
      turn_id: 'turn_1',
      status: 'completed',
      snapshot_source: 'runtime',
      snapshot: null,
    };
    const apiClient = {
      getConversationTurnRuntimeContextByTurn: vi.fn(async () => terminalSnapshot),
      getConversationLatestTurnRuntimeContext: vi.fn(async () => terminalSnapshot),
      getConversationTurnMessagesByTurn: vi.fn(async () => []),
      getConversationTurnMessages: vi.fn(async () => []),
    };

    const recovered = await recoverMessagesAfterRealtimeTerminalEvent(
      apiClient,
      set,
      state,
      {
        sessionId: 'session_1',
        turnId: 'turn_1',
        tempAssistantMessageId: 'assistant_temp_1',
        tempUserId: 'user_temp_1',
      },
    );

    expect(recovered).toBe(false);
    expect(state.loadMessages).toHaveBeenCalledWith('session_1');
  });
});
