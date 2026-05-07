import { describe, expect, it } from 'vitest';

import type { Message } from '../../types';
import { buildVisibleMessageState, parseMessageForList } from './derivedData';

const buildAssistant = (overrides: Partial<Message> = {}): Message => ({
  id: 'assistant-1',
  sessionId: 'session-1',
  role: 'assistant',
  content: '你好',
  status: 'completed',
  createdAt: new Date('2026-05-07T10:00:01.000Z'),
  metadata: {
    conversation_turn_id: 'turn-1',
    historyFinalForTurnId: 'turn-1',
    historyFinalForUserMessageId: 'user-1',
  },
  ...overrides,
});

const buildUser = (overrides: Partial<Message> = {}): Message => ({
  id: 'user-1',
  sessionId: 'session-1',
  role: 'user',
  content: '你好啊',
  status: 'completed',
  createdAt: new Date('2026-05-07T10:00:00.000Z'),
  metadata: {
    conversation_turn_id: 'turn-1',
    historyProcess: {
      hasProcess: true,
      toolCallCount: 0,
      thinkingCount: 2,
      processMessageCount: 0,
      userMessageId: 'user-1',
      turnId: 'turn-1',
      finalAssistantMessageId: 'assistant-2',
      expanded: false,
      loaded: false,
      loading: false,
    },
  },
  ...overrides,
});

describe('buildVisibleMessageState', () => {
  it('keeps only one final assistant visible for the same turn during transient local duplicates', () => {
    const messages: Message[] = [
      buildUser(),
      buildAssistant({
        id: 'assistant-1',
        content: '在的！',
        createdAt: new Date('2026-05-07T10:00:01.000Z'),
      }),
      buildAssistant({
        id: 'assistant-2',
        content: '在的！',
        createdAt: new Date('2026-05-07T10:00:02.000Z'),
        metadata: {
          conversation_turn_id: 'turn-1',
          historyFinalForTurnId: 'turn-1',
          historyFinalForUserMessageId: 'user-1',
        },
      }),
    ];

    const state = buildVisibleMessageState(messages.map(parseMessageForList));
    const assistantMessages = state.visibleMessages.filter((message) => message.role === 'assistant');

    expect(assistantMessages).toHaveLength(1);
    expect(assistantMessages[0]?.id).toBe('assistant-2');
  });
});
