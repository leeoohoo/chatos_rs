import { describe, expect, it } from 'vitest';

import {
  applyToolEndResultsToMessage,
  applyToolStreamDataToMessage,
} from './toolEvents';
import type {
  RawToolResultPayload,
  StreamingMessage,
  StreamingMessageMetadata,
} from './types';

const buildMessage = (): StreamingMessage => ({
  id: 'assistant_1',
  sessionId: 'session_1',
  role: 'assistant',
  content: '',
  status: 'streaming',
  createdAt: new Date('2026-04-17T00:00:00.000Z'),
  metadata: {
    toolCalls: [
      {
        id: 'call_1',
        messageId: 'assistant_1',
        name: 'web_research',
        arguments: { query: '今日新闻 最新' },
        createdAt: new Date('2026-04-17T00:00:00.000Z'),
      },
    ],
    contentSegments: [],
  } as StreamingMessageMetadata,
});

describe('toolEvents', () => {
  it('keeps structured degraded tool end results out of hard error state', () => {
    const message = buildMessage();
    const payload: RawToolResultPayload = {
      tool_call_id: 'call_1',
      success: false,
      is_error: false,
      result: {
        success: false,
        status: 'degraded',
        warning: 'duckduckgo timed out',
      },
      content: 'Web research completed with degraded search coverage.',
    };

    applyToolEndResultsToMessage(message, [payload]);

    const toolCall = (message.metadata?.toolCalls || [])[0] as Record<string, unknown>;
    expect(toolCall.error).toBeUndefined();
    expect(toolCall.completed).toBe(true);
    expect(toolCall.result).toEqual(payload.result);
  });

  it('keeps structured degraded tool stream payloads out of hard error state', () => {
    const message = buildMessage();
    const payload: RawToolResultPayload = {
      tool_call_id: 'call_1',
      success: false,
      is_error: false,
      result: {
        success: false,
        status: 'degraded',
      },
      content: 'Search providers were unavailable, returning fallback bundle.',
      is_stream: false,
    };

    const handled = applyToolStreamDataToMessage(message, payload);

    const toolCall = (message.metadata?.toolCalls || [])[0] as Record<string, unknown>;
    expect(handled).toBe(true);
    expect(toolCall.error).toBeUndefined();
    expect(toolCall.result).toBe('Search providers were unavailable, returning fallback bundle.');
    expect(toolCall.completed).toBe(true);
  });

  it('still marks transport-level tool failures as errors', () => {
    const message = buildMessage();
    const payload: RawToolResultPayload = {
      tool_call_id: 'call_1',
      success: false,
      is_error: true,
      content: '工具执行失败: request timed out',
    };

    applyToolEndResultsToMessage(message, [payload]);

    const toolCall = (message.metadata?.toolCalls || [])[0] as Record<string, unknown>;
    expect(toolCall.error).toBe('工具执行失败: request timed out');
    expect(toolCall.completed).toBe(true);
  });
});
