// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { Message } from '../../types';
import type { ChatStoreDraft } from '../../lib/store/types';
import {
  applyTaskRunnerCallbackRealtimeUpdate,
  applyTaskRunnerRealtimeError,
} from './useChatStreamRealtimeBridge';

const message = (role: 'user' | 'assistant', turnId: string): Message => ({
  id: `${role}-${turnId}`,
  sessionId: 'session-1',
  role,
  content: role,
  status: 'completed',
  createdAt: new Date('2026-07-20T00:00:00Z'),
  metadata: { conversation_turn_id: turnId },
});

const activeState = (): ChatStoreDraft => ({
  currentSessionId: 'session-1',
  messages: [],
  sessionMessagesCache: {},
  sessionChatState: {
    'session-1': {
      isLoading: true,
      isStreaming: false,
      isStopping: false,
      streamingPhase: null,
      streamingMessageId: null,
      activeTurnId: 'turn-active',
      streamingPreviewText: '',
      streamingTransport: 'realtime',
      runtimeContextRefreshNonce: 0,
    },
  },
  isLoading: true,
  isStreaming: false,
  streamingMessageId: null,
} as unknown as ChatStoreDraft);

describe('chat stream realtime persisted message reconciliation', () => {
  it('keeps the active turn running when only its user message is persisted', () => {
    const state = activeState();

    applyTaskRunnerCallbackRealtimeUpdate(
      state,
      'session-1',
      message('user', 'turn-active'),
      null,
    );

    expect(state.sessionChatState['session-1']).toMatchObject({
      isLoading: true,
      activeTurnId: 'turn-active',
      streamingTransport: 'realtime',
    });
  });

  it('settles the active turn when its assistant terminal message is persisted', () => {
    const state = activeState();

    applyTaskRunnerCallbackRealtimeUpdate(
      state,
      'session-1',
      message('user', 'turn-active'),
      message('assistant', 'turn-active'),
    );

    expect(state.sessionChatState['session-1']).toMatchObject({
      isLoading: false,
      isStreaming: false,
      activeTurnId: null,
      streamingTransport: null,
    });
  });

  it('does not let an older callback settle a newer active turn', () => {
    const state = activeState();

    applyTaskRunnerCallbackRealtimeUpdate(
      state,
      'session-1',
      message('user', 'turn-old'),
      message('assistant', 'turn-old'),
    );

    expect(state.sessionChatState['session-1']).toMatchObject({
      isLoading: true,
      activeTurnId: 'turn-active',
      streamingTransport: 'realtime',
    });
  });

  it('does not let an older terminal error settle a newer active turn', () => {
    const state = activeState();

    applyTaskRunnerRealtimeError(
      state,
      'session-1',
      'older turn failed',
      'turn-old',
      'failed',
    );

    expect(state.sessionChatState['session-1']).toMatchObject({
      isLoading: true,
      activeTurnId: 'turn-active',
      streamingTransport: 'realtime',
    });
    expect(state.isLoading).toBe(true);
    expect(state.error).not.toBe('older turn failed');
  });

  it('settles the active turn when its terminal error arrives', () => {
    const state = activeState();

    applyTaskRunnerRealtimeError(
      state,
      'session-1',
      'active turn failed',
      'turn-active',
      'failed',
    );

    expect(state.sessionChatState['session-1']).toMatchObject({
      isLoading: false,
      activeTurnId: null,
      streamingTransport: null,
    });
    expect(state.isLoading).toBe(false);
    expect(state.error).toBe('active turn failed');
  });

  it('treats cancellation as a normal terminal state without an error banner', () => {
    const state = activeState();

    applyTaskRunnerRealtimeError(
      state,
      'session-1',
      'Chat turn cancelled',
      'turn-active',
      'cancelled',
    );

    expect(state.sessionChatState['session-1']).toMatchObject({
      isLoading: false,
      activeTurnId: null,
    });
    expect(state.messages.some((item) => item.role === 'assistant')).toBe(false);
    expect(state.error).toBeNull();
  });

  it('redacts provider secrets from terminal errors', () => {
    const state = activeState();

    applyTaskRunnerRealtimeError(
      state,
      'session-1',
      'status 500 Internal Server Error: {"api_key":"secret","internal_trace":"trace-1"}',
      'turn-active',
      'failed',
    );

    expect(state.error).toBe('模型服务调用失败，请稍后重试或检查模型配置。');
    expect(JSON.stringify(state.messages)).not.toContain('secret');
    expect(JSON.stringify(state.messages)).not.toContain('internal_trace');
  });
});
