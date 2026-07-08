// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { SessionMessageResponse } from '../api/client/types';
import {
  getConversationTurnId,
  isTaskRunnerAsyncMessage,
  normalizeRawMessages,
  normalizeTurnId,
} from './messages';

describe('domain/messages', () => {
  it('normalizes assistant tool calls and tool results from mixed payload shapes', () => {
    const rawMessages = [
      {
        id: 'assistant_1',
        conversation_id: 'session_from_payload',
        role: 'assistant',
        content: 'Final answer',
        reasoning: 'Need to inspect the repository first',
        metadata: JSON.stringify({
          attachments: [
            {
              id: 'att_1',
              name: 'report.txt',
              url: '/files/report.txt',
              mime: 'text/plain',
              size: 12,
            },
          ],
        }),
        tool_calls: [
          {
            id: 'tool_1',
            tool_name: 'workspace_search',
            arguments: '{"query":"TODO"}',
            completed: true,
          },
        ],
        created_at: '2026-04-23T10:00:00.000Z',
      },
      {
        id: 'tool_message_1',
        conversation_id: 'session_from_payload',
        role: 'tool',
        tool_call_id: 'tool_1',
        content: 'tool fallback content',
        metadata: JSON.stringify({
          structured_result: {
            hits: [{ path: 'src/app.ts', line: 3 }],
          },
        }),
        created_at: '2026-04-23T10:00:01.000Z',
      },
    ] as unknown as SessionMessageResponse[];

    const [message] = normalizeRawMessages(rawMessages, 'fallback_session');
    const toolCalls = message.metadata?.toolCalls || [];
    const contentSegments = message.metadata?.contentSegments || [];
    const attachments = message.metadata?.attachments || [];

    expect(message.sessionId).toBe('session_from_payload');
    expect(toolCalls).toHaveLength(1);
    expect(toolCalls[0]?.id).toBe('tool_1');
    expect(toolCalls[0]?.name).toBe('workspace_search');
    expect(toolCalls[0]?.arguments).toEqual({ query: 'TODO' });
    expect(toolCalls[0]?.result).toEqual({
      hits: [{ path: 'src/app.ts', line: 3 }],
    });
    expect(contentSegments.map((segment) => segment.type)).toEqual(['thinking', 'tool_call', 'text']);
    expect(attachments).toHaveLength(1);
    expect(attachments[0]?.type).toBe('file');
  });

  it('accepts explicit content segments and trims conversation turn ids', () => {
    const rawMessages = [
      {
        id: 'assistant_2',
        role: 'assistant',
        content: 'ignored fallback',
        metadata: {
          conversation_turn_id: '  turn_123  ',
          content_segments: [
            { type: 'text', content: 'Segment text' },
            { type: 'tool', tool_call_id: 'tool_2' },
          ],
        },
        created_at: '2026-04-23T11:00:00.000Z',
      },
    ] as unknown as SessionMessageResponse[];

    const [message] = normalizeRawMessages(rawMessages, 'session_a');

    expect(message.metadata?.contentSegments).toEqual([
      { type: 'text', content: 'Segment text' },
      { type: 'tool_call', toolCallId: 'tool_2' },
    ]);
    expect(getConversationTurnId(message)).toBe('turn_123');
    expect(normalizeTurnId('  turn_123  ')).toBe('turn_123');
    expect(normalizeTurnId(null)).toBe('');
  });

  it('tolerates null metadata without breaking normalization', () => {
    const rawMessages = [
      {
        id: 'assistant_3',
        role: 'assistant',
        content: 'plain content',
        metadata: null,
        created_at: '2026-04-23T12:00:00.000Z',
      },
    ] as unknown as SessionMessageResponse[];

    const [message] = normalizeRawMessages(rawMessages, 'session_b');

    expect(message.metadata).toMatchObject({
      toolCalls: undefined,
      contentSegments: [{ type: 'text', content: 'plain content' }],
    });
    expect(message.content).toBe('plain content');
    expect(message.status).toBe('completed');
  });

  it('normalizes serialized Responses output_text parts before building render segments', () => {
    const rawMessages = [
      {
        id: 'assistant_tool_calls',
        role: 'assistant',
        content: '{"type":"output_text","annotations":[],"logprobs":[],"text":""}',
        metadata: {
          response_status: 'tool_calls',
        },
        tool_calls: [
          {
            id: 'call_cancel_1',
            name: 'task_runner_service_cancel_task',
            arguments: '{}',
          },
        ],
        created_at: '2026-04-23T12:05:00.000Z',
      },
      {
        id: 'assistant_text_part',
        role: 'assistant',
        content: JSON.stringify([
          { type: 'output_text', text: 'visible ' },
          { type: 'output_text', text: 'answer' },
        ]),
        metadata: null,
        created_at: '2026-04-23T12:06:00.000Z',
      },
    ] as unknown as SessionMessageResponse[];

    const [toolCallMessage, textMessage] = normalizeRawMessages(rawMessages, 'session_b');

    expect(toolCallMessage.content).toBe('');
    expect(toolCallMessage.metadata?.contentSegments).toEqual([
      { type: 'tool_call', toolCallId: 'call_cancel_1' },
    ]);
    expect(textMessage.content).toBe('visible answer');
    expect(textMessage.metadata?.contentSegments).toEqual([
      { type: 'text', content: 'visible answer' },
    ]);
  });

  it('treats only task runner assistant outputs as task runner async messages', () => {
    expect(isTaskRunnerAsyncMessage({
      messageMode: 'task_runner_async_plan',
      metadata: null,
    })).toBe(true);

    expect(isTaskRunnerAsyncMessage({
      messageMode: 'task_runner_callback',
      metadata: null,
    })).toBe(true);

    expect(isTaskRunnerAsyncMessage({
      metadata: {
        task_runner_async: {
          mode: 'contact_async',
          overall_status: 'processing',
        },
      },
    })).toBe(false);
  });
});
