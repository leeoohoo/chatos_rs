import { describe, expect, it, vi } from 'vitest';

import type { TurnRuntimeSnapshotLookupResponse } from '../../../api/client/types';
import type { Message } from '../../../../types';
import type { ChatStoreDraft } from '../../types';
import {
  canUseLocalTerminalAssistant,
  findLocalTurnAssistantCandidate,
  shouldReloadMessagesAfterTerminalState,
} from './persistedTurnMessages';
import { recoverStreamingTurnBySnapshot } from './turnRecovery';

const buildAssistant = (overrides: Partial<Message> = {}): Message => ({
  id: 'assistant_1',
  sessionId: 'session_1',
  role: 'assistant',
  content: 'final answer',
  status: 'completed',
  createdAt: new Date('2026-05-07T10:00:01.000Z'),
  metadata: {
    conversation_turn_id: 'turn_1',
    historyFinalForTurnId: 'turn_1',
    historyFinalForUserMessageId: 'user_1',
    historyDraftUserMessage: {
      id: 'temp_user_1',
      content: 'hello',
      createdAt: '2026-05-07T10:00:00.000Z',
    },
    contentSegments: [{ type: 'text', content: 'final answer' }],
    toolCalls: [],
  },
  ...overrides,
});

const buildUser = (overrides: Partial<Message> = {}): Message => ({
  id: 'temp_user_1',
  sessionId: 'session_1',
  role: 'user',
  content: 'hello',
  status: 'completed',
  createdAt: new Date('2026-05-07T10:00:00.000Z'),
  metadata: {
    conversation_turn_id: 'turn_1',
    historyProcess: {
      hasProcess: false,
      toolCallCount: 0,
      thinkingCount: 0,
      processMessageCount: 0,
      userMessageId: 'temp_user_1',
      turnId: 'turn_1',
      finalAssistantMessageId: 'temp_assistant_1',
      expanded: false,
      loaded: false,
      loading: false,
    },
  },
  ...overrides,
});

const buildStreamingRecoveryState = (): ChatStoreDraft => ({
  currentSessionId: 'session_1',
  isLoading: true,
  isStreaming: true,
  streamingMessageId: 'temp_assistant_1',
  messages: [
    buildUser(),
    buildAssistant({
      id: 'temp_assistant_1',
      status: 'streaming',
      content: 'stale draft',
      metadata: {
        conversation_turn_id: 'turn_1',
        historyFinalForTurnId: 'turn_1',
        historyFinalForUserMessageId: 'temp_user_1',
        historyDraftUserMessage: {
          id: 'temp_user_1',
          content: 'hello',
          createdAt: '2026-05-07T10:00:00.000Z',
        },
        contentSegments: [{ type: 'text', content: 'stale draft' }],
        toolCalls: [],
      },
    }),
  ] as Message[],
  sessionChatState: {
    session_1: {
      isLoading: true,
      isStreaming: true,
      isStopping: true,
      streamingMessageId: 'temp_assistant_1',
      activeTurnId: 'turn_1',
      streamingPreviewText: 'stale draft',
      streamingTransport: 'realtime',
      runtimeContextRefreshNonce: 0,
    },
  },
  sessionStreamingMessageDrafts: {
    session_1: buildAssistant({
      id: 'temp_assistant_1',
      status: 'streaming',
      content: 'stale draft',
      metadata: {
        conversation_turn_id: 'turn_1',
        historyFinalForTurnId: 'turn_1',
        historyFinalForUserMessageId: 'temp_user_1',
        historyDraftUserMessage: {
          id: 'temp_user_1',
          content: 'hello',
          createdAt: '2026-05-07T10:00:00.000Z',
        },
        contentSegments: [{ type: 'text', content: 'stale draft' }],
        toolCalls: [],
      },
    }),
  },
}) as unknown as ChatStoreDraft;

describe('persistedTurnMessages', () => {
  it('skips whole-session reload when a local terminal assistant already safely closes the turn', () => {
    const tempAssistant = buildAssistant({
      id: 'temp_assistant_1',
      status: 'error',
      content: 'local error bubble',
      metadata: {
        conversation_turn_id: 'turn_1',
        historyFinalForTurnId: 'turn_1',
        historyFinalForUserMessageId: 'user_1',
        historyDraftUserMessage: {
          id: 'temp_user_1',
          content: 'hello',
          createdAt: '2026-05-07T10:00:00.000Z',
        },
        contentSegments: [{ type: 'text', content: 'local error bubble' }],
        requestError: 'network failed',
      },
    });

    expect(
      shouldReloadMessagesAfterTerminalState(
        {
          messages: [
            buildUser(),
            tempAssistant,
          ] as Message[],
        } as Pick<ChatStoreDraft, 'messages'>,
        'temp_assistant_1',
        'temp_user_1',
        {
          allowLocalTerminalAssistant: true,
        },
      ),
    ).toBe(false);
  });

  it('prefers the persisted local assistant over the old temp assistant candidate', () => {
    const candidate = findLocalTurnAssistantCandidate(
      [
        buildAssistant({
          id: 'temp_assistant_1',
          status: 'error',
          content: '',
          metadata: {
            conversation_turn_id: 'turn_1',
            historyFinalForTurnId: 'turn_1',
            historyFinalForUserMessageId: 'temp_user_1',
            historyDraftUserMessage: {
              id: 'temp_user_1',
              content: 'hello',
              createdAt: '2026-05-07T10:00:00.000Z',
            },
            contentSegments: [{ type: 'text', content: '' }],
          },
        }),
        buildAssistant(),
      ] as Message[],
      'temp_assistant_1',
      'temp_user_1',
      'turn_1',
    );

    expect(candidate?.id).toBe('assistant_1');
    expect(
      canUseLocalTerminalAssistant(candidate, {
        expectedTurnId: 'turn_1',
        tempUserId: 'temp_user_1',
        requireTerminalStatus: true,
      }),
    ).toBe(true);
  });
});

describe('turnRecovery', () => {
  it('uses local terminal assistant recovery when snapshot is terminal but turn messages are still empty', async () => {
    const set = vi.fn((updater: (state: ChatStoreDraft) => void) => {
      updater(state);
    });
    const snapshot: TurnRuntimeSnapshotLookupResponse = {
      conversation_id: 'session_1',
      turn_id: 'turn_1',
      status: 'completed',
      snapshot_source: 'runtime',
      snapshot: null,
    };
    const apiClient = {
      getConversationTurnRuntimeContextByTurn: vi.fn().mockResolvedValue(snapshot),
      getConversationLatestTurnRuntimeContext: vi.fn().mockResolvedValue(snapshot),
      getConversationTurnMessagesByTurn: vi.fn().mockResolvedValue([]),
      getConversationTurnMessages: vi.fn().mockResolvedValue([]),
    };
    const state = {
      currentSessionId: 'session_1',
      isLoading: true,
      isStreaming: true,
      streamingMessageId: 'temp_assistant_1',
      messages: [
        buildUser(),
        buildAssistant({
          id: 'temp_assistant_1',
          status: 'streaming',
          metadata: {
            conversation_turn_id: 'turn_1',
            historyFinalForTurnId: 'turn_1',
            historyFinalForUserMessageId: 'temp_user_1',
            historyDraftUserMessage: {
              id: 'temp_user_1',
              content: 'hello',
              createdAt: '2026-05-07T10:00:00.000Z',
            },
            contentSegments: [{ type: 'text', content: 'streamed final answer' }],
            toolCalls: [],
          },
        }),
      ] as Message[],
      sessionChatState: {
        session_1: {
          isLoading: true,
          isStreaming: true,
          isStopping: false,
          streamingMessageId: 'temp_assistant_1',
          activeTurnId: 'turn_1',
          streamingPreviewText: 'streamed final answer',
          streamingTransport: 'realtime',
          runtimeContextRefreshNonce: 0,
        },
      },
      sessionStreamingMessageDrafts: {
        session_1: buildAssistant({
          id: 'temp_assistant_1',
          status: 'streaming',
          metadata: {
            conversation_turn_id: 'turn_1',
            historyFinalForTurnId: 'turn_1',
            historyFinalForUserMessageId: 'temp_user_1',
            historyDraftUserMessage: {
              id: 'temp_user_1',
              content: 'hello',
              createdAt: '2026-05-07T10:00:00.000Z',
            },
            contentSegments: [{ type: 'text', content: 'streamed final answer' }],
            toolCalls: [],
          },
        }),
      },
    } as unknown as ChatStoreDraft;

    const result = await recoverStreamingTurnBySnapshot({
      apiClient,
      set,
      sessionId: 'session_1',
      turnId: 'turn_1',
      tempAssistantMessageId: 'temp_assistant_1',
      tempUserId: 'temp_user_1',
      preferredUserMessageId: 'temp_user_1',
    });

    expect(result.recovered).toBe(true);
    expect(result.terminal).toBe(true);
    expect(state.messages.find((message) => message.id === 'temp_assistant_1')?.status).toBe('completed');
    expect(state.sessionChatState.session_1.isStreaming).toBe(false);
    expect(state.sessionStreamingMessageDrafts.session_1).toBeNull();
  });

  it('keeps streaming state when a running snapshot is inactive but still fresh', async () => {
    const set = vi.fn((updater: (state: ChatStoreDraft) => void) => {
      updater(state);
    });
    const nowIso = new Date().toISOString();
    const snapshot: TurnRuntimeSnapshotLookupResponse = {
      conversation_id: 'session_1',
      turn_id: 'turn_1',
      status: 'running',
      snapshot_source: 'runtime',
      active_in_runtime: false,
      snapshot: {
        id: 'snapshot_1',
        conversation_id: 'session_1',
        user_id: 'user_1',
        turn_id: 'turn_1',
        status: 'running',
        snapshot_source: 'runtime',
        snapshot_version: 1,
        captured_at: nowIso,
        updated_at: nowIso,
      },
    };
    const apiClient = {
      getConversationTurnRuntimeContextByTurn: vi.fn().mockResolvedValue(snapshot),
      getConversationLatestTurnRuntimeContext: vi.fn().mockResolvedValue(snapshot),
      getConversationTurnMessagesByTurn: vi.fn().mockResolvedValue([]),
      getConversationTurnMessages: vi.fn().mockResolvedValue([]),
    };
    const state = buildStreamingRecoveryState();

    const result = await recoverStreamingTurnBySnapshot({
      apiClient,
      set,
      sessionId: 'session_1',
      turnId: 'turn_1',
      tempAssistantMessageId: 'temp_assistant_1',
      tempUserId: 'temp_user_1',
      preferredUserMessageId: 'temp_user_1',
    });

    expect(result.recovered).toBe(true);
    expect(result.terminal).toBe(false);
    expect(state.sessionChatState.session_1.isStreaming).toBe(true);
    expect(state.sessionChatState.session_1.isStopping).toBe(false);
    expect(state.sessionChatState.session_1.activeTurnId).toBe('turn_1');
    expect(state.sessionStreamingMessageDrafts.session_1?.id).toBe('temp_assistant_1');
  });

  it('clears stale streaming state when a running snapshot is inactive for a long time', async () => {
    const set = vi.fn((updater: (state: ChatStoreDraft) => void) => {
      updater(state);
    });
    const staleUpdatedAtIso = new Date(Date.now() - (11 * 60 * 1000)).toISOString();
    const snapshot: TurnRuntimeSnapshotLookupResponse = {
      conversation_id: 'session_1',
      turn_id: 'turn_1',
      status: 'running',
      snapshot_source: 'runtime',
      active_in_runtime: false,
      snapshot: {
        id: 'snapshot_2',
        conversation_id: 'session_1',
        user_id: 'user_1',
        turn_id: 'turn_1',
        status: 'running',
        snapshot_source: 'runtime',
        snapshot_version: 1,
        captured_at: staleUpdatedAtIso,
        updated_at: staleUpdatedAtIso,
      },
    };
    const apiClient = {
      getConversationTurnRuntimeContextByTurn: vi.fn().mockResolvedValue(snapshot),
      getConversationLatestTurnRuntimeContext: vi.fn().mockResolvedValue(snapshot),
      getConversationTurnMessagesByTurn: vi.fn().mockResolvedValue([]),
      getConversationTurnMessages: vi.fn().mockResolvedValue([]),
    };
    const state = buildStreamingRecoveryState();

    const result = await recoverStreamingTurnBySnapshot({
      apiClient,
      set,
      sessionId: 'session_1',
      turnId: 'turn_1',
      tempAssistantMessageId: 'temp_assistant_1',
      tempUserId: 'temp_user_1',
      preferredUserMessageId: 'temp_user_1',
    });

    expect(result.recovered).toBe(true);
    expect(result.terminal).toBe(true);
    expect(state.sessionChatState.session_1.isStreaming).toBe(false);
    expect(state.sessionChatState.session_1.isStopping).toBe(false);
    expect(state.sessionChatState.session_1.activeTurnId).toBeNull();
    expect(state.sessionStreamingMessageDrafts.session_1).toBeNull();
  });
});
