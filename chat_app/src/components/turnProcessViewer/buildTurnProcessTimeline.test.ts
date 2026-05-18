import { describe, expect, it } from 'vitest';

import type { Message } from '../../types';
import { buildTurnProcessTimeline } from './buildTurnProcessTimeline';

type MessageToolCall = NonNullable<NonNullable<Message['metadata']>['toolCalls']>[number];

const buildAssistant = (overrides: Partial<Message> = {}): Message => ({
  id: 'assistant-1',
  sessionId: 'session-1',
  role: 'assistant',
  content: '',
  status: 'completed',
  createdAt: new Date('2026-05-18T10:00:00.000Z'),
  metadata: {},
  ...overrides,
});

describe('buildTurnProcessTimeline', () => {
  it('builds timeline items from persisted process assistant content', () => {
    const processMessage = buildAssistant({
      id: 'assistant-process-1',
      metadata: {
        contentSegments: [
          { type: 'thinking', content: '先看日志' },
          { type: 'tool_call', toolCallId: 'tool-call-1', content: '' as never },
        ],
        toolCalls: [{
          id: 'tool-call-1',
          messageId: 'assistant-process-1',
          name: 'process_log',
          arguments: {},
          createdAt: new Date('2026-05-18T10:00:01.000Z'),
          result: 'ok',
        }],
      },
    });

    const timeline = buildTurnProcessTimeline({
      processMessages: [processMessage],
      fallbackAssistantMessage: null,
    });

    expect(timeline).toHaveLength(2);
    expect(timeline[0]).toMatchObject({
      kind: 'thinking',
      text: '先看日志',
    });
    expect(timeline[1]).toMatchObject({
      kind: 'tool_call',
      toolCall: {
        id: 'tool-call-1',
        name: 'process_log',
      },
    });
  });

  it('falls back to streaming final assistant segments when no persisted process messages exist', () => {
    const fallbackAssistant = buildAssistant({
      id: 'assistant-streaming-1',
      status: 'streaming',
      metadata: {
        contentSegments: [
          { type: 'thinking', content: '正在分析' },
          { type: 'tool_call', toolCallId: 'tool-call-2', content: '' as never },
          { type: 'text', content: '最终回答草稿' },
        ],
        toolCalls: [{
          id: 'tool-call-2',
          messageId: 'assistant-streaming-1',
          name: 'run_command',
          arguments: { cmd: 'ls' },
          createdAt: new Date('2026-05-18T10:00:02.000Z'),
          ...({
            streamLog: 'file-a\nfile-b',
            completed: false,
          } as Record<string, unknown>),
        } as MessageToolCall],
      },
    });

    const timeline = buildTurnProcessTimeline({
      processMessages: [],
      fallbackAssistantMessage: fallbackAssistant,
    });

    expect(timeline).toHaveLength(2);
    expect(timeline[0]).toMatchObject({
      kind: 'thinking',
      text: '正在分析',
      isStreaming: true,
    });
    expect(timeline[1]).toMatchObject({
      kind: 'tool_call',
      toolCall: {
        id: 'tool-call-2',
        name: 'run_command',
      },
      streamLog: 'file-a\nfile-b',
      completed: false,
    });
  });

  it('merges streaming final assistant process into persisted timeline when both exist', () => {
    const processMessage = buildAssistant({
      id: 'assistant-process-merged-1',
      metadata: {
        contentSegments: [
          { type: 'thinking', content: '先检查上下文' },
        ],
      },
    });

    const fallbackAssistant = buildAssistant({
      id: 'assistant-streaming-merged-1',
      status: 'streaming',
      createdAt: new Date('2026-05-18T10:00:05.000Z'),
      metadata: {
        contentSegments: [
          { type: 'thinking', content: '继续整理最终结果' },
          { type: 'tool_call', toolCallId: 'tool-call-merged-1', content: '' as never },
        ],
        toolCalls: [{
          id: 'tool-call-merged-1',
          messageId: 'assistant-streaming-merged-1',
          name: 'run_command',
          arguments: { cmd: 'pwd' },
          createdAt: new Date('2026-05-18T10:00:06.000Z'),
          ...({
            streamLog: '/workspace',
            completed: false,
          } as Record<string, unknown>),
        } as MessageToolCall],
      },
    });

    const timeline = buildTurnProcessTimeline({
      processMessages: [processMessage],
      fallbackAssistantMessage: fallbackAssistant,
    });

    expect(timeline).toHaveLength(3);
    expect(timeline.map((item) => item.kind)).toEqual([
      'thinking',
      'thinking',
      'tool_call',
    ]);
    expect(timeline[1]).toMatchObject({
      kind: 'thinking',
      text: '继续整理最终结果',
      isStreaming: true,
    });
    expect(timeline[2]).toMatchObject({
      kind: 'tool_call',
      streamLog: '/workspace',
      completed: false,
    });
  });
});
