// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { Message } from '../../types';
import { normalizeRawMessages } from '../../lib/domain/messages';
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
  });

  it('filters assistant tool-call carrier messages from the main visible message list', () => {
    const messages: Message[] = [
      buildUser({
        id: 'user-tool-carrier-1',
        metadata: {
          conversation_turn_id: 'turn-tool-carrier-1',
        },
      }),
      buildAssistant({
        id: 'assistant-tool-carrier-1',
        content: '{"type":"output_text","annotations":[],"logprobs":[],"text":""}',
        metadata: {
          conversation_turn_id: 'turn-tool-carrier-1',
          toolCalls: [{
            id: 'tool-call-carrier-1',
            messageId: 'assistant-tool-carrier-1',
            name: 'task_runner_service_cancel_task',
            arguments: {},
            createdAt: new Date('2026-05-07T10:00:01.000Z'),
          }],
          contentSegments: [
            { type: 'tool_call', toolCallId: 'tool-call-carrier-1', content: '' as never },
          ],
        },
      }),
      buildAssistant({
        id: 'assistant-final-tool-carrier-1',
        content: '已处理完成',
        metadata: {
          conversation_turn_id: 'turn-tool-carrier-1',
          historyFinalForUserMessageId: 'user-tool-carrier-1',
          historyFinalForTurnId: 'turn-tool-carrier-1',
        },
      }),
    ];

    const state = buildVisibleMessageState(messages.map(parseMessageForList));

    expect(state.visibleMessages.map((message) => message.id)).toEqual([
      'user-tool-carrier-1',
      'assistant-final-tool-carrier-1',
    ]);
    expect(state.assistantToolCallById.has('tool-call-carrier-1')).toBe(true);
  });

  it('filters the live task-runner async plan payload while tools are still running', () => {
    const messages = normalizeRawMessages([
      {
        id: 'user-live-plan-1',
        conversation_id: 'session-1',
        role: 'user',
        content: '刚才好像失败了，你重新试一下吧',
        status: 'completed',
        created_at: '2026-07-22T07:15:05.943434Z',
        message_mode: 'task_runner_async_plan',
        metadata: {
          conversation_turn_id: 'turn-live-plan-1',
          task_runner_async: {
            mode: 'contact_async',
            overall_status: 'processing',
          },
        },
      },
      {
        id: 'assistant-live-plan-tools-1',
        conversation_id: 'session-1',
        role: 'assistant',
        content: '',
        status: 'completed',
        created_at: '2026-07-22T07:15:19.309907Z',
        message_mode: 'task_runner_async_plan',
        metadata: {
          conversation_turn_id: 'turn-live-plan-1',
          response_status: 'tool_calls',
          task_runner_async: {
            mode: 'contact_async',
            message_kind: 'plan_summary',
          },
          reasoning: 'Planning parallel task status listing',
          toolCalls: [
            {
              id: 'call-live-plan-1',
              type: 'function',
              function: {
                name: 'task_runner_service_list_tasks',
                arguments: '{}',
              },
            },
          ],
        },
      },
    ] as never, 'session-1');

    const state = buildVisibleMessageState(messages.map(parseMessageForList));

    expect(state.visibleMessages.map((message) => message.id)).toEqual([
      'user-live-plan-1',
    ]);
    expect(state.assistantToolCallById.has('call-live-plan-1')).toBe(true);
  });

  it('keeps task-runner callback messages for the same source turn in chronological order', () => {
    const messages: Message[] = [
      buildUser({
        id: 'user-callback-order-1',
        content: '帮我跑一下这个任务',
        createdAt: new Date('2026-08-11T09:07:00.000Z'),
        metadata: {
          conversation_turn_id: 'turn-callback-order-1',
        },
      }),
      buildAssistant({
        id: 'assistant-callback-earlier',
        content: '任务第一次执行失败',
        createdAt: new Date('2026-08-11T09:16:00.000Z'),
        messageMode: 'task_runner_callback',
        metadata: {
          task_runner_async: {
            mode: 'contact_async',
            message_kind: 'task_terminal_update',
            source_user_message_id: 'user-callback-order-1',
            source_turn_id: 'turn-callback-order-1',
          },
        },
      }),
      buildAssistant({
        id: 'assistant-callback-later',
        content: '任务稍后完成',
        createdAt: new Date('2026-08-11T09:28:00.000Z'),
        messageMode: 'task_runner_callback',
        metadata: {
          task_runner_async: {
            mode: 'contact_async',
            message_kind: 'task_terminal_update',
            source_user_message_id: 'user-callback-order-1',
            source_turn_id: 'turn-callback-order-1',
          },
        },
      }),
    ];

    const state = buildVisibleMessageState(messages.map(parseMessageForList));

    expect(state.visibleMessages.map((message) => message.id)).toEqual([
      'user-callback-order-1',
      'assistant-callback-earlier',
      'assistant-callback-later',
    ]);
  });

  it('filters intermediate assistant text when the same response also carries tool calls', () => {
    const messages: Message[] = [
      buildUser({
        id: 'user-tool-text-1',
        metadata: {
          conversation_turn_id: 'turn-tool-text-1',
        },
      }),
      buildAssistant({
        id: 'assistant-tool-text-1',
        content: '我先检查仍在排队的任务，然后继续处理。',
        metadata: {
          conversation_turn_id: 'turn-tool-text-1',
          response_status: 'completed',
          toolCalls: [{
            id: 'tool-call-text-1',
            messageId: 'assistant-tool-text-1',
            name: 'task_runner_service_list_tasks',
            arguments: {},
            createdAt: new Date('2026-05-07T10:00:01.000Z'),
          }],
          contentSegments: [
            { type: 'thinking', content: '先检查任务状态' },
            { type: 'tool_call', toolCallId: 'tool-call-text-1', content: '' as never },
            { type: 'text', content: '我先检查仍在排队的任务，然后继续处理。' },
          ],
        },
      }),
      buildAssistant({
        id: 'assistant-final-tool-text-1',
        content: '已生成完整开发计划。',
        metadata: {
          conversation_turn_id: 'turn-tool-text-1',
          historyFinalForUserMessageId: 'user-tool-text-1',
          historyFinalForTurnId: 'turn-tool-text-1',
        },
      }),
    ];

    const state = buildVisibleMessageState(messages.map(parseMessageForList));

    expect(state.visibleMessages.map((message) => message.id)).toEqual([
      'user-tool-text-1',
      'assistant-final-tool-text-1',
    ]);
    expect(state.assistantToolCallById.has('tool-call-text-1')).toBe(true);
  });

  it('filters hidden tool messages from the main visible message list', () => {
    const messages: Message[] = [
      buildUser({
        id: 'user-tool-only-1',
        metadata: {
          conversation_turn_id: 'turn-tool-only-1',
          historyProcess: {
            hasProcess: false,
            toolCallCount: 0,
            thinkingCount: 0,
            processMessageCount: 0,
            userMessageId: 'user-tool-only-1',
            turnId: 'turn-tool-only-1',
            finalAssistantMessageId: 'assistant-final-tool-only-1',
            expanded: false,
            loaded: true,
            loading: false,
          },
        },
      }),
      {
        id: 'tool-only-1',
        sessionId: 'session-1',
        role: 'tool',
        content: 'tool output only',
        status: 'completed',
        createdAt: new Date('2026-05-07T10:00:01.000Z'),
        metadata: {
          toolCallId: 'tool-call-only-1',
          historyProcessUserMessageId: 'user-tool-only-1',
          historyProcessTurnId: 'turn-tool-only-1',
          hidden: true,
        },
      },
      buildAssistant({
        id: 'assistant-final-tool-only-1',
        content: '最终回复',
        createdAt: new Date('2026-05-07T10:00:02.000Z'),
        metadata: {
          conversation_turn_id: 'turn-tool-only-1',
          historyFinalForUserMessageId: 'user-tool-only-1',
          historyFinalForTurnId: 'turn-tool-only-1',
        },
      }),
    ];

    const state = buildVisibleMessageState(messages.map(parseMessageForList));

    expect(state.visibleMessages.map((message) => message.id)).toEqual([
      'user-tool-only-1',
      'assistant-final-tool-only-1',
    ]);
  });

  it('anchors task runner receipts to their source turn even when they complete after a newer user message', () => {
    const messages: Message[] = [
      buildUser({
        id: 'user-source',
        content: '先执行一个后台任务',
        createdAt: new Date('2026-05-07T10:00:00.000Z'),
        metadata: {
          conversation_turn_id: 'turn-source',
        },
      }),
      buildAssistant({
        id: 'assistant-source',
        content: '任务已经安排后台执行。',
        createdAt: new Date('2026-05-07T10:00:01.000Z'),
        metadata: {
          conversation_turn_id: 'turn-source',
          historyFinalForUserMessageId: 'user-source',
          historyFinalForTurnId: 'turn-source',
        },
      }),
      buildUser({
        id: 'user-newer',
        content: '我又发了一条新消息',
        createdAt: new Date('2026-05-07T10:00:02.000Z'),
        metadata: {
          conversation_turn_id: 'turn-newer',
        },
      }),
      buildAssistant({
        id: 'task-runner-receipt',
        content: '后台任务完成回执',
        messageMode: 'task_run_receipt',
        createdAt: new Date('2026-05-07T10:00:03.000Z'),
        metadata: {
          task_runner_async: {
            message_kind: 'task_terminal_update',
            source_turn_id: 'turn-source',
            last_task_id: 'lc_async_task_1',
            overall_status: 'completed',
          },
        },
      }),
    ];

    const state = buildVisibleMessageState(messages.map(parseMessageForList));

    expect(state.visibleMessages.map((message) => message.id)).toEqual([
      'user-source',
      'assistant-source',
      'task-runner-receipt',
      'user-newer',
    ]);
  });
});
