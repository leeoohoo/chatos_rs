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

  it('replaces the optimistic assistant draft with the persisted terminal assistant message', () => {
    const state = buildState();
    const set = (updater: (draft: ChatStoreDraft) => void) => updater(state);

    applyRealtimeTerminalMessages(set, {
      sessionId: 'session_1',
      turnId: 'turn_1',
      tempAssistantMessageId: 'assistant_temp_1',
      tempUserId: 'user_temp_1',
    }, {
      persistedUserMessage: {
        ...buildUser(),
        id: 'user_1',
      },
      persistedAssistantMessage: {
        ...buildAssistant({
          id: 'assistant_1',
          content: 'final answer',
          status: 'completed',
          metadata: {
            conversation_turn_id: 'turn_1',
            historyFinalForTurnId: 'turn_1',
            historyFinalForUserMessageId: 'user_1',
            contentSegments: [{ type: 'text', content: 'final answer' }],
          },
        }),
      },
    });

    expect(state.messages.some((message) => message.id === 'assistant_temp_1')).toBe(false);
    expect(state.messages.find((message) => message.id === 'assistant_1')?.content).toBe('final answer');
    expect(state.messages.find((message) => message.id === 'assistant_1')?.status).toBe('completed');
    expect(state.sessionChatState.session_1.isStreaming).toBe(false);
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

  it('recovers a non-active session back to streaming when terminal event arrives but runtime snapshot is still running', async () => {
    const state = {
      ...buildState(),
      currentSessionId: 'other_session',
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
      loadMessages: vi.fn(async () => {}),
    } as ChatStoreDraft;
    const set = (updater: (draft: ChatStoreDraft) => void) => updater(state);
    const runningSnapshot: TurnRuntimeSnapshotLookupResponse = {
      conversation_id: 'session_1',
      turn_id: 'turn_1',
      status: 'running',
      snapshot_source: 'runtime',
      active_in_runtime: true,
      snapshot: {
        id: 'snapshot_1',
        conversation_id: 'session_1',
        user_id: 'user_1',
        turn_id: 'turn_1',
        status: 'running',
        snapshot_source: 'runtime',
        snapshot_version: 1,
        captured_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      },
    };
    const apiClient = {
      getConversationTurnRuntimeContextByTurn: vi.fn(async () => runningSnapshot),
      getConversationLatestTurnRuntimeContext: vi.fn(async () => runningSnapshot),
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

    expect(state.sessionChatState.session_1.isStreaming).toBe(true);
    expect(state.sessionChatState.session_1.activeTurnId).toBe('turn_1');
    expect(state.sessionStreamingMessageDrafts.session_1?.id).toBe('assistant_temp_1');
    expect(state.loadMessages).not.toHaveBeenCalled();
  });

  it('recovers from session draft without loadMessages when visible messages are empty', async () => {
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

    expect(recovered).toBe(true);
    expect(state.sessionChatState.session_1.isStreaming).toBe(false);
    expect(state.loadMessages).not.toHaveBeenCalled();
  });

  it('settles terminal state locally when snapshot recovery has neither visible messages nor streaming draft', async () => {
    const state = {
      ...buildState(),
      messages: [],
      sessionStreamingMessageDrafts: {
        session_1: null,
      },
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

    expect(recovered).toBe(true);
    expect(state.sessionChatState.session_1.isStreaming).toBe(false);
    expect(state.loadMessages).not.toHaveBeenCalled();
  });
});
