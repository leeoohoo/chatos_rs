// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { Message } from '../../types';
import type { UserMessageTurn } from './types';
import {
  buildTimelineItems,
  selectTurnProcessMessages,
} from './ConversationProcessTimelineModel';

const message = (
  id: string,
  role: Message['role'],
  content: string,
  metadata: Message['metadata'] = {},
  createdAt = new Date('2026-07-21T00:00:00Z'),
): Message => ({
  id,
  sessionId: 'session-1',
  role,
  content,
  status: 'completed',
  createdAt,
  metadata,
});

const turnItem = (): UserMessageTurn => ({
  turnId: 'turn-1',
  userMessage: message('user-1', 'user', 'hello'),
  finalAssistantMessage: message('assistant-final', 'assistant', 'done'),
  hasProcess: true,
  processMessageCount: 2,
  thinkingCount: 1,
  toolCallCount: 1,
  taskState: {
    hasTask: false,
    running: false,
    label: null,
    runningCount: 0,
  },
});

describe('ConversationProcessTimelineModel', () => {
  it('selects unmarked local process messages by turn boundaries', () => {
    const item = turnItem();
    const processAssistant = message(
      'assistant-process',
      'assistant',
      'thinking',
      {},
      new Date('2026-07-21T00:00:01Z'),
    );
    const processTool = message(
      'tool-process',
      'tool',
      'result',
      {},
      new Date('2026-07-21T00:00:02Z'),
    );

    expect(selectTurnProcessMessages([
      item.userMessage,
      processAssistant,
      processTool,
      item.finalAssistantMessage!,
    ], item).map((value) => value.id)).toEqual([
      'assistant-process',
      'tool-process',
    ]);
  });

  it('keeps explicitly marked cloud process messages even without boundary messages', () => {
    const item = turnItem();
    const process = message('cloud-process', 'assistant', '', {
      historyProcessLoaded: true,
    });

    expect(selectTurnProcessMessages([process], item)).toEqual([process]);
  });

  it('splits thinking segments and tool calls into separate events', () => {
    const process = message('assistant-process', 'assistant', '', {
      contentSegments: [
        { type: 'thinking', content: '先检查目录' },
        { type: 'thinking', content: '再读取文件' },
      ],
      toolCalls: [{
        id: 'call-1',
        messageId: 'assistant-process',
        name: 'code_maintainer_read_read_file_raw',
        arguments: { path: 'src/model.ts' },
        result: 'file content',
        createdAt: new Date('2026-07-21T00:00:00Z'),
      }],
    });

    const timeline = buildTimelineItems([process]);
    expect(timeline.map((item) => item.type)).toEqual([
      'model',
      'model',
      'tool_call',
    ]);
  });
});
