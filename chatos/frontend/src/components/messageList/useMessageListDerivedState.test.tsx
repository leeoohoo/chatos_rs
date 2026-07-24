// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
// @vitest-environment jsdom

import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { Message } from '../../types';
import { useMessageListDerivedState } from './useMessageListDerivedState';

describe('useMessageListDerivedState', () => {
  it('reclassifies a live assistant when tool metadata is mutated in place', () => {
    const userMessage: Message = {
      id: 'user-live-mutation-1',
      sessionId: 'session-1',
      role: 'user',
      content: '重新试一下',
      status: 'completed',
      createdAt: new Date('2026-07-22T07:15:05.000Z'),
      metadata: {
        conversation_turn_id: 'turn-live-mutation-1',
      },
    };
    const assistantMessage: Message = {
      id: 'assistant-live-mutation-1',
      sessionId: 'session-1',
      role: 'assistant',
      content: '',
      status: 'completed',
      createdAt: new Date('2026-07-22T07:15:19.000Z'),
      metadata: {
        conversation_turn_id: 'turn-live-mutation-1',
        task_runner_async: {
          mode: 'contact_async',
          message_kind: 'plan_summary',
        },
        toolCalls: [],
        contentSegments: [],
      },
    };
    const messages = [userMessage, assistantMessage];
    const { result, rerender } = renderHook(
      ({ items }) => useMessageListDerivedState(items),
      { initialProps: { items: messages } },
    );

    expect(result.current.dedupedVisibleMessages.map((message) => message.id)).toEqual([
      'user-live-mutation-1',
      'assistant-live-mutation-1',
    ]);

    assistantMessage.metadata!.toolCalls!.push({
      id: 'call-live-mutation-1',
      messageId: assistantMessage.id,
      name: 'task_runner_service_list_tasks',
      arguments: {},
      createdAt: assistantMessage.createdAt,
    });
    assistantMessage.metadata!.contentSegments!.push(
      { type: 'thinking', content: 'Planning task status listing' },
      { type: 'tool_call', toolCallId: 'call-live-mutation-1', content: '' },
    );

    rerender({ items: messages });

    expect(result.current.dedupedVisibleMessages.map((message) => message.id)).toEqual([
      'user-live-mutation-1',
    ]);
    expect(result.current.assistantToolCallById.has('call-live-mutation-1')).toBe(true);
  });
});
