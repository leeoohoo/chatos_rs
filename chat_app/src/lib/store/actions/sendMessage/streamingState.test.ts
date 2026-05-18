import { describe, expect, it, vi } from 'vitest';

import type { Message } from '../../../../types';
import type { ChatStoreDraft, ChatStoreSet } from '../../types';
import { createStreamingMessageStateHelpers } from './streamingState';

const buildUserMessage = (): Message => ({
  id: 'user_1',
  sessionId: 'session_1',
  role: 'user',
  content: 'hello',
  status: 'completed',
  createdAt: new Date('2026-05-18T10:00:00.000Z'),
  metadata: {
    historyProcess: {
      hasProcess: false,
      toolCallCount: 0,
      thinkingCount: 0,
      unavailableToolCount: 0,
      processMessageCount: 0,
      userMessageId: 'user_1',
      turnId: 'turn_1',
      finalAssistantMessageId: 'assistant_temp_1',
      expanded: false,
      loaded: false,
      loading: false,
    },
  },
});

const buildStreamingAssistant = (): Message => ({
  id: 'assistant_temp_1',
  sessionId: 'session_1',
  role: 'assistant',
  content: '',
  status: 'streaming',
  createdAt: new Date('2026-05-18T10:00:01.000Z'),
  metadata: {
    contentSegments: [],
    toolCalls: [],
    currentSegmentIndex: 0,
  },
});

describe('createStreamingMessageStateHelpers', () => {
  it('merges snapshot-style thinking updates without duplicating prefixes', () => {
    const state = {
      currentSessionId: 'session_1',
      messages: [
        buildUserMessage(),
        buildStreamingAssistant(),
      ],
      sessionStreamingMessageDrafts: {},
    } as unknown as ChatStoreDraft;

    const set: ChatStoreSet = vi.fn((updater) => {
      updater(state);
    });

    const helpers = createStreamingMessageStateHelpers({
      set,
      currentSessionId: 'session_1',
      tempAssistantMessage: buildStreamingAssistant(),
      tempUserId: 'user_1',
      conversationTurnId: 'turn_1',
      streamedTextRef: { value: '' },
    });

    helpers.appendThinkingToStreamingMessage('Continuing task execution');
    helpers.appendThinkingToStreamingMessage(
      'Continuing task execution I need to respond to "OK 继续吧"',
    );

    const assistant = state.messages.find((message) => message.id === 'assistant_temp_1');
    const thinkingSegments = (assistant?.metadata?.contentSegments || [])
      .filter((segment) => segment.type === 'thinking');

    expect(thinkingSegments).toHaveLength(1);
    expect(thinkingSegments[0]?.content).toBe(
      'Continuing task execution I need to respond to "OK 继续吧"',
    );
    expect(state.messages[0]?.metadata?.historyProcess).toMatchObject({
      thinkingCount: 1,
      processMessageCount: 1,
    });
  });
});
