// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import React from 'react';
import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ApiClientProvider } from '../../../lib/api/ApiClientContext';
import type ApiClient from '../../../lib/api/client';
import type { SessionMessageResponse } from '../../../lib/api/client/types';
import type { Message } from '../../../types';
import {
  isRequirementExecutionPlannerTimelineMessage,
  useRequirementExecutionPlannerTimeline,
} from './useRequirementExecutionPlannerTimeline';

const message = (role: Message['role'], metadata?: Message['metadata']): Message => ({
  id: `${role}-1`,
  sessionId: 'session-1',
  role,
  content: role === 'assistant' ? '最终模型输出' : '',
  status: 'completed',
  createdAt: new Date('2026-08-17T08:00:00Z'),
  metadata,
});

describe('requirement execution planner timeline selection', () => {
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
});
