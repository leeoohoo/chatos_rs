// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';
import type { MessageTaskRunnerRunEvent } from '../../lib/api/client/types';
import { buildTimelineSummary } from '../userMessages/ConversationProcessTimelineModel';
import { buildRunProcessTimelineItems } from './runProcessTimelineModel';

const event = (
  id: string,
  eventType: string,
  payload?: unknown,
  message?: string,
): MessageTaskRunnerRunEvent => ({
  id,
  run_id: 'run-1',
  event_type: eventType,
  payload,
  message,
  created_at: `2026-07-21T08:00:${id.padStart(2, '0')}Z`,
});

describe('buildRunProcessTimelineItems', () => {
  it('splits a tool batch into actions and merges each result by call id', () => {
    const items = buildRunProcessTimelineItems([
      event('1', 'tools_start', [
        {
          id: 'call-read',
          type: 'function',
          function: {
            name: 'code_maintainer_read_read_file_raw',
            arguments: JSON.stringify({ path: 'src/model.ts' }),
          },
        },
        {
          id: 'call-search',
          type: 'function',
          function: {
            name: 'code_maintainer_read_search_text',
            arguments: JSON.stringify({ path: 'src', pattern: 'completed' }),
          },
        },
      ]),
      event('2', 'tool_stream', {
        tool_call_id: 'call-search',
        name: 'code_maintainer_read_search_text',
        success: true,
        is_error: false,
        is_stream: false,
        content: 'src/model.ts:42',
      }),
      event('3', 'tool_stream', {
        tool_call_id: 'call-read',
        name: 'code_maintainer_read_read_file_raw',
        success: true,
        is_error: false,
        is_stream: false,
        content: 'export const completed = true;',
      }),
    ]);

    const toolItems = items.filter((item) => item.type === 'tool_call');
    expect(toolItems).toHaveLength(2);
    expect(toolItems[0]).toMatchObject({
      hasResult: true,
      result: 'export const completed = true;',
      status: 'completed',
      toolCall: {
        id: 'call-read',
        name: 'code_maintainer_read_read_file_raw',
        arguments: JSON.stringify({ path: 'src/model.ts' }),
      },
    });
    expect(toolItems[1]).toMatchObject({
      hasResult: true,
      result: 'src/model.ts:42',
      status: 'completed',
      toolCall: { id: 'call-search' },
    });
    expect(buildTimelineSummary(items)).toMatchObject({
      toolCall: 2,
      toolResult: 2,
      error: 0,
    });
  });

  it('groups consecutive model text and keeps an unfinished tool pending', () => {
    const items = buildRunProcessTimelineItems([
      event('1', 'model_request', { model: 'gpt-5.4' }, '即将发起模型请求'),
      event('2', 'thinking', { text: '先检查目录' }),
      event('3', 'thinking', { text: '再读取文件' }),
      event('4', 'chunk', { text: '处理中' }),
      event('5', 'tools_start', [{
        id: 'call-read',
        function: {
          name: 'code_maintainer_read_read_file_raw',
          arguments: { path: 'src/model.ts' },
        },
      }]),
      event('6', 'tool_stream', {
        tool_call_id: 'call-read',
        is_stream: true,
        content: 'partial output',
      }),
    ]);

    expect(items.map((item) => item.type)).toEqual([
      'model',
      'model',
      'model',
      'tool_call',
    ]);
    expect(items[1]).toMatchObject({
      label: '模型思考',
      content: '先检查目录\n\n再读取文件',
    });
    expect(items[3]).toMatchObject({
      hasResult: false,
      status: 'pending',
    });
  });

  it('keeps an unpaired final tool result visible for diagnosis', () => {
    const items = buildRunProcessTimelineItems([
      event('1', 'tool_stream', {
        tool_call_id: 'call-missing',
        success: false,
        is_error: true,
        is_stream: false,
        content: 'network unavailable',
      }),
    ]);

    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({
      type: 'tool_result',
      callId: 'call-missing',
      error: 'network unavailable',
      status: 'error',
    });
  });
});
