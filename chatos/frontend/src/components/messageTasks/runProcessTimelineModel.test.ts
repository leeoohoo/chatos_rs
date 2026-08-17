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

  it('marks a background terminal start complete when the tool returns', () => {
    const items = buildRunProcessTimelineItems([
      event('1', 'tools_start', [{
        id: 'call-background',
        function: {
          name: 'sandbox_terminal_controller_execute_command',
          arguments: { command: 'npm run build:frontend', background: true },
        },
      }]),
      event('2', 'tool_stream', {
        tool_call_id: 'call-background',
        name: 'sandbox_terminal_controller_execute_command',
        success: true,
        is_error: false,
        is_stream: false,
        result: {
          background: true,
          busy: true,
          process_id: 'process-1',
        },
      }),
    ]);

    expect(items[0]).toMatchObject({
      type: 'tool_call',
      hasResult: true,
      status: 'completed',
    });
  });

  it('matches legacy results without call ids by the tool name', () => {
    const items = buildRunProcessTimelineItems([
      event('1', 'tools_start', [{
        id: 'call-legacy',
        function: {
          name: 'code_maintainer_read_read_file_raw',
          arguments: { path: 'README.md' },
        },
      }]),
      event('2', 'tool_stream', {
        name: 'code_maintainer_read_read_file_raw',
        success: true,
        is_error: false,
        is_stream: false,
        content: 'legacy result',
      }),
    ]);

    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({
      type: 'tool_call',
      hasResult: true,
      result: 'legacy result',
      status: 'completed',
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

  it('uses a truncation preview instead of showing a false empty result', () => {
    const items = buildRunProcessTimelineItems([
      event('1', 'tool_stream', {
        truncated: true,
        original_bytes: 64000,
        preview: '{"path":"src/large.rs","content":"visible preview"}',
      }),
    ]);

    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({
      type: 'tool_result',
      hasResult: true,
      result: '{"path":"src/large.rs","content":"visible preview"}',
    });
  });

  it('falls back to content when a structured result is empty', () => {
    const items = buildRunProcessTimelineItems([
      event('1', 'tools_start', [{
        id: 'call-read',
        function: { name: 'read_file', arguments: '{}' },
      }]),
      event('2', 'tool_stream', {
        tool_call_id: 'call-read',
        success: true,
        is_stream: false,
        result: {},
        content: 'visible content',
      }),
    ]);

    expect(items[0]).toMatchObject({
      type: 'tool_call',
      hasResult: true,
      result: 'visible content',
    });
  });

  it('marks an empty terminal result complete instead of pending', () => {
    const items = buildRunProcessTimelineItems([
      event('1', 'tools_start', [{
        id: 'call-empty',
        function: { name: 'terminal_controller_process_wait', arguments: '{}' },
      }]),
      event('2', 'tool_stream', {
        tool_call_id: 'call-empty',
        name: 'terminal_controller_process_wait',
        success: true,
        is_error: false,
        is_stream: false,
        result: null,
        content: '',
      }),
    ]);

    expect(items[0]).toMatchObject({
      type: 'tool_call',
      hasResult: false,
      status: 'completed',
    });
  });

  it('does not create a card for a genuinely empty unpaired result', () => {
    const items = buildRunProcessTimelineItems([
      event('1', 'tool_stream', {
        success: true,
        is_stream: false,
        result: {},
        content: '',
      }),
    ]);

    expect(items).toEqual([]);
  });

  it('keeps task lifecycle failures visible', () => {
    const items = buildRunProcessTimelineItems([
      event('1', 'queued', undefined, 'task run queued'),
      event('2', 'running', undefined, '任务开始执行'),
      event('3', 'dispatch_failed', { project_id: 'project-1' }, '任务派发失败'),
    ]);

    expect(items).toHaveLength(3);
    expect(items[0]).toMatchObject({ type: 'model', label: '任务入队' });
    expect(items[1]).toMatchObject({ type: 'model', label: '任务开始执行' });
    expect(items[2]).toMatchObject({
      type: 'tool_result',
      status: 'error',
      error: '任务派发失败',
    });
  });

  it('collapses identical repeated lifecycle failures without merging different errors', () => {
    const items = buildRunProcessTimelineItems([
      event('1', 'running', { worker: 'worker-1' }, '任务开始执行'),
      event('2', 'dispatch_failed', { status: 'unavailable' }, '任务派发失败'),
      event('3', 'running', { worker: 'worker-1' }, '任务开始执行'),
      event('4', 'dispatch_failed', { status: 'unavailable' }, '任务派发失败'),
      event('5', 'dispatch_failed', { status: 'rejected' }, '任务派发被拒绝'),
    ]);

    expect(items).toHaveLength(3);
    expect(items[0]).toMatchObject({
      type: 'model',
      label: '任务开始执行',
      repeatCount: 2,
    });
    expect(items[1]).toMatchObject({
      type: 'tool_result',
      error: '任务派发失败',
      repeatCount: 2,
    });
    expect(items[2]).toMatchObject({
      type: 'tool_result',
      error: '任务派发被拒绝',
    });
  });
});
