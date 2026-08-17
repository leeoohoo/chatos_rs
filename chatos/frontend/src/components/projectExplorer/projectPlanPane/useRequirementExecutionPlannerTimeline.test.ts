// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import React from 'react';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiClientProvider } from '../../../lib/api/ApiClientContext';
import type ApiClient from '../../../lib/api/client';
import type { SessionMessageResponse } from '../../../lib/api/client/types';
import type { Message } from '../../../types';
import type { RealtimeChatStreamPayloadWrapper } from '../../../lib/realtime/types';
import {
  isRequirementExecutionPlannerTimelineMessage,
  useRequirementExecutionPlannerTimeline,
} from './useRequirementExecutionPlannerTimeline';

const realtimeMock = vi.hoisted(() => ({
  onEvent: null as null | ((payload: RealtimeChatStreamPayloadWrapper, eventName: string) => void),
}));

vi.mock('../../../lib/realtime/useConversationChatStreamRealtime', () => ({
  useConversationChatStreamRealtime: (options: {
    onEvent: (payload: RealtimeChatStreamPayloadWrapper, eventName: string) => void;
  }) => {
    realtimeMock.onEvent = options.onEvent;
  },
}));

const message = (role: Message['role'], metadata?: Message['metadata']): Message => ({
  id: `${role}-1`,
  sessionId: 'session-1',
  role,
  content: role === 'assistant' ? '最终模型输出' : '',
  status: 'completed',
  createdAt: new Date('2026-08-17T08:00:00Z'),
  metadata,
});

const realtimePayload = (
  streamType: string,
  raw: RealtimeChatStreamPayloadWrapper['raw'],
  overrides: Partial<RealtimeChatStreamPayloadWrapper> = {},
): RealtimeChatStreamPayloadWrapper => ({
  kind: 'chat_stream',
  conversation_id: 'conversation-1',
  conversation_turn_id: 'turn-1',
  user_message_id: 'user-1',
  stream_type: streamType,
  raw: { type: streamType, timestamp: '2026-08-17T08:00:00Z', ...raw },
  ...overrides,
});

const emptyClient = () => ({
  getConversationTurnMessagesByTurn: vi.fn(async () => []),
}) as unknown as ApiClient;

const apiWrapper = (client: ApiClient) => (
  ({ children }: { children: React.ReactNode }) => (
    React.createElement(ApiClientProvider, { children, client })
  )
);

describe('requirement execution planner timeline selection', () => {
  beforeEach(() => {
    realtimeMock.onEvent = null;
  });

  it('includes final assistant output even when it is not marked as a process placeholder', () => {
    expect(isRequirementExecutionPlannerTimelineMessage(message('assistant'))).toBe(true);
  });

  it('includes loaded process records and excludes the planner user record', () => {
    expect(isRequirementExecutionPlannerTimelineMessage(message('tool', {
      historyProcessLoaded: true,
    }))).toBe(true);
    expect(isRequirementExecutionPlannerTimelineMessage(message('user'))).toBe(false);
  });

  it('does not render records from the previous execution group while the new group loads', async () => {
    let resolveNew: (messages: SessionMessageResponse[]) => void = () => undefined;
    const newMessages = new Promise<SessionMessageResponse[]>((resolve) => {
      resolveNew = resolve;
    });
    const oldMessages: SessionMessageResponse[] = [
      { id: 'assistant-old', role: 'assistant', content: '旧模型输出' },
      { id: 'tool-old', role: 'tool', content: '旧工具结果' },
    ];
    const getConversationTurnMessagesByTurn = vi.fn(
      async (_conversationId: string, turnId: string) => (
        turnId === 'turn-old' ? oldMessages : newMessages
      ),
    );
    const client = { getConversationTurnMessagesByTurn } as unknown as ApiClient;
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      React.createElement(ApiClientProvider, { children, client })
    );
    const { result, rerender } = renderHook(
      ({ turnId }: { turnId: string }) => useRequirementExecutionPlannerTimeline({
        active: true,
        conversationId: 'conversation-1',
        turnId,
        userMessageId: turnId,
      }),
      { initialProps: { turnId: 'turn-old' }, wrapper },
    );

    await waitFor(() => expect(result.current.processMessageCount).toBe(2));

    rerender({ turnId: 'turn-new' });

    expect(result.current.processMessageCount).toBe(0);
    expect(result.current.loading).toBe(true);

    await act(async () => {
      resolveNew([]);
      await newMessages;
    });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.processMessageCount).toBe(0);
  });

  it('shows the streaming model request immediately before any Memory Engine record exists', async () => {
    const { result } = renderHook(() => useRequirementExecutionPlannerTimeline({
      active: true,
      conversationId: 'conversation-1',
      turnId: 'turn-1',
      userMessageId: 'user-1',
    }), { wrapper: apiWrapper(emptyClient()) });

    await waitFor(() => expect(realtimeMock.onEvent).not.toBeNull());
    act(() => {
      realtimeMock.onEvent?.(realtimePayload('turn_phase', {
        data: {
          phase: 'model_request',
          iteration: 1,
          request_attempt: 1,
          stream: true,
          model: 'gpt-5.5',
          input_item_count: 20,
          tool_count: 8,
          read_timeout_seconds: 300,
        },
      }), 'chat.turn.phase');
    });

    expect(result.current.processMessageCount).toBe(1);
    expect(result.current.items[0]).toMatchObject({
      label: '模型请求',
      type: 'model',
    });
    expect(result.current.items[0]?.type === 'model' && result.current.items[0].content)
      .toContain('流读取超时 300 秒');
  });

  it('ignores realtime events from another turn and merges consecutive stream chunks', async () => {
    const { result } = renderHook(() => useRequirementExecutionPlannerTimeline({
      active: true,
      conversationId: 'conversation-1',
      turnId: 'turn-1',
      userMessageId: 'user-1',
    }), { wrapper: apiWrapper(emptyClient()) });

    await waitFor(() => expect(realtimeMock.onEvent).not.toBeNull());
    act(() => {
      realtimeMock.onEvent?.(realtimePayload('chunk', { content: '错误 turn' }, {
        conversation_turn_id: 'turn-other',
      }), 'chat.turn.delta');
      realtimeMock.onEvent?.(realtimePayload('chunk', { content: '模型' }), 'chat.turn.delta');
      realtimeMock.onEvent?.(realtimePayload('chunk', { content: '输出' }), 'chat.turn.delta');
    });

    expect(result.current.processMessageCount).toBe(1);
    expect(result.current.items[0]).toMatchObject({
      content: '模型输出',
      label: '模型输出',
      type: 'model',
    });
  });

  it('updates a realtime tool call from pending to completed', async () => {
    const { result } = renderHook(() => useRequirementExecutionPlannerTimeline({
      active: true,
      conversationId: 'conversation-1',
      turnId: 'turn-1',
      userMessageId: 'user-1',
    }), { wrapper: apiWrapper(emptyClient()) });

    await waitFor(() => expect(realtimeMock.onEvent).not.toBeNull());
    act(() => {
      realtimeMock.onEvent?.(realtimePayload('tools_start', {
        data: {
          tool_calls: [{ id: 'call-1', function: { name: 'list_dir', arguments: '{}' } }],
        },
      }), 'chat.tool.started');
      realtimeMock.onEvent?.(realtimePayload('tools_end', {
        data: {
          tool_results: [{
            tool_call_id: 'call-1',
            name: 'list_dir',
            success: true,
            is_error: false,
            content: 'done',
          }],
        },
      }), 'chat.tool.completed');
    });

    expect(result.current.items[0]).toMatchObject({
      hasResult: true,
      status: 'completed',
      type: 'tool_call',
    });
  });
});
