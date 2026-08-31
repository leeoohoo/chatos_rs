// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import {
  compactHistoryToUserMessageTurns,
  getConversationUserMessageTurns,
} from './sessions';

describe('workspace session api helpers', () => {
  it('loads user-message turns from the same compact-history endpoint as the chat pane', async () => {
    const request = vi.fn().mockResolvedValue({
      items: [],
      has_more: false,
      next_before: null,
    });

    await getConversationUserMessageTurns(request as never, 'conv-1', {
      limit: 10,
      before: 'turn-9',
    });

    expect(request).toHaveBeenCalledTimes(1);
    expect(request).toHaveBeenCalledWith(
      '/conversations/conv-1/compact-history?limit=10&before=turn-9',
    );
  });

  it('derives the user-message sidebar turn from compact history metadata', () => {
    const response = compactHistoryToUserMessageTurns({
      items: [
        {
          id: 'user-1',
          role: 'user',
          content: 'hello',
          metadata: {
            conversation_turn_id: 'turn-1',
            historyProcess: {
              turnId: 'turn-1',
              finalAssistantMessageId: 'assistant-1',
              hasProcess: true,
              toolCallCount: 2,
              thinkingCount: 1,
              processMessageCount: 3,
            },
          },
        },
        {
          id: 'assistant-1',
          role: 'assistant',
          content: 'world',
          metadata: {
            historyFinalForUserMessageId: 'user-1',
            historyFinalForTurnId: 'turn-1',
          },
        },
      ],
      has_more: true,
      next_before: 'turn-0',
    });

    expect(response).toEqual({
      items: [{
        turn_id: 'turn-1',
        user_message: expect.objectContaining({ id: 'user-1' }),
        final_assistant_message: expect.objectContaining({ id: 'assistant-1' }),
        has_process: true,
        tool_call_count: 2,
        thinking_count: 1,
        process_message_count: 3,
      }],
      has_more: true,
      next_before: 'turn-0',
    });
  });
});
