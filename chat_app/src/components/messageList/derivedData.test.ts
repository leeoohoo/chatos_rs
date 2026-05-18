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

  it('keeps the final assistant visible when the same turn also has many process messages and tool traces', () => {
    const messages: Message[] = [
      buildUser({
        metadata: {
          conversation_turn_id: 'turn-9',
          historyProcess: {
            hasProcess: true,
            toolCallCount: 2,
            thinkingCount: 2,
            processMessageCount: 3,
            userMessageId: 'user-9',
            turnId: 'turn-9',
            finalAssistantMessageId: 'assistant-final-9',
            expanded: false,
            loaded: true,
            loading: false,
          },
        },
        id: 'user-9',
        content: '帮我查一下并执行',
      }),
      buildAssistant({
        id: 'assistant-process-1',
        content: '',
        createdAt: new Date('2026-05-07T10:00:01.000Z'),
        metadata: {
          conversation_turn_id: 'turn-9',
          historyProcessUserMessageId: 'user-9',
          historyProcessTurnId: 'turn-9',
          hidden: true,
          contentSegments: [
            { type: 'thinking', content: '先分析需求' },
            { type: 'tool_call', toolCallId: 'tool-call-1', content: '' as never },
          ],
          toolCalls: [{
            id: 'tool-call-1',
            messageId: 'assistant-process-1',
            name: 'search_docs',
            arguments: {},
            createdAt: new Date('2026-05-07T10:00:01.000Z'),
          }],
        },
      }),
      {
        id: 'tool-result-1',
        sessionId: 'session-1',
        role: 'tool',
        content: '搜索结果',
        status: 'completed',
        createdAt: new Date('2026-05-07T10:00:02.000Z'),
        metadata: {
          toolCallId: 'tool-call-1',
          historyProcessUserMessageId: 'user-9',
          historyProcessTurnId: 'turn-9',
          hidden: true,
        },
      },
      buildAssistant({
        id: 'assistant-process-2',
        content: '',
        createdAt: new Date('2026-05-07T10:00:03.000Z'),
        metadata: {
          conversation_turn_id: 'turn-9',
          historyProcessUserMessageId: 'user-9',
          historyProcessTurnId: 'turn-9',
          hidden: true,
          contentSegments: [
            { type: 'thinking', content: '继续执行' },
            { type: 'tool_call', toolCallId: 'tool-call-2', content: '' as never },
          ],
          toolCalls: [{
            id: 'tool-call-2',
            messageId: 'assistant-process-2',
            name: 'run_task',
            arguments: {},
            createdAt: new Date('2026-05-07T10:00:03.000Z'),
          }],
        },
      }),
      buildAssistant({
        id: 'assistant-final-9',
        content: '已经查完并执行好了',
        createdAt: new Date('2026-05-07T10:00:04.000Z'),
        metadata: {
          conversation_turn_id: 'turn-9',
          historyFinalForTurnId: 'turn-9',
          historyFinalForUserMessageId: 'user-9',
          contentSegments: [
            { type: 'text', content: '已经查完并执行好了' },
          ],
        },
      }),
    ];

    const state = buildVisibleMessageState(messages.map(parseMessageForList));

    expect(state.visibleMessages.map((message) => message.id)).toEqual(['user-9', 'assistant-final-9']);
    expect(state.derivedProcessStatsByUserId.get('user-9')).toMatchObject({
      hasProcess: true,
      toolCallCount: 2,
      thinkingCount: 2,
    });
  });

  it('filters inline process assistant messages from the main visible message list', () => {
    const messages: Message[] = [
      buildUser({
        id: 'user-inline-1',
        metadata: {
          conversation_turn_id: 'turn-inline-1',
          historyProcess: {
            hasProcess: true,
            toolCallCount: 1,
            thinkingCount: 1,
            processMessageCount: 1,
            userMessageId: 'user-inline-1',
            turnId: 'turn-inline-1',
            finalAssistantMessageId: 'assistant-final-inline-1',
            expanded: true,
            loaded: true,
            loading: false,
          },
        },
      }),
      buildAssistant({
        id: 'assistant-inline-process-1',
        content: '',
        metadata: {
          conversation_turn_id: 'turn-inline-1',
          historyProcessUserMessageId: 'user-inline-1',
          historyProcessTurnId: 'turn-inline-1',
          hidden: false,
          contentSegments: [
            { type: 'thinking', content: '分析一下' },
          ],
        },
      }),
      buildAssistant({
        id: 'assistant-final-inline-1',
        content: '最终答案',
        metadata: {
          conversation_turn_id: 'turn-inline-1',
          historyFinalForUserMessageId: 'user-inline-1',
          historyFinalForTurnId: 'turn-inline-1',
        },
      }),
    ];

    const state = buildVisibleMessageState(messages.map(parseMessageForList));

    expect(state.visibleMessages.map((message) => message.id)).toEqual([
      'user-inline-1',
      'assistant-final-inline-1',
    ]);
    expect(state.derivedProcessStatsByUserId.get('user-inline-1')).toMatchObject({
      hasProcess: true,
      thinkingCount: 1,
      processMessageCount: 1,
    });
  });
});
