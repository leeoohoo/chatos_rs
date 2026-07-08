// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';
import type { MessageTaskRunnerRunEvent } from '../../lib/api/client/types';
import { buildRunEventTimelineEntries } from './runEventTimelineUtils';

const createEvent = (
  id: string,
  eventType: string,
  payload?: unknown,
): MessageTaskRunnerRunEvent => ({
  id,
  run_id: 'run-1',
  event_type: eventType,
  payload,
  created_at: `2026-06-15T08:32:0${id}Z`,
});

describe('buildRunEventTimelineEntries', () => {
  it('groups consecutive chunk events into one timeline entry', () => {
    const entries = buildRunEventTimelineEntries([
      createEvent('1', 'chunk', { text: 'hello' }),
      createEvent('2', 'chunk', { text: 'world' }),
      createEvent('3', 'model_request', { model: 'gpt-5.4' }),
    ]);

    expect(entries).toHaveLength(2);
    expect(entries[0]).toMatchObject({
      kind: 'group',
      eventType: 'chunk',
      summary: '已聚合 2 段内容 · 12 字',
    });
    expect(entries[0]?.events).toHaveLength(2);
    expect(entries[0]?.aggregatedText).toContain('hello');
    expect(entries[0]?.aggregatedText).toContain('world');
    expect(entries[1]).toMatchObject({
      kind: 'single',
      eventType: 'model_request',
    });
  });

  it('keeps different tool streams in separate groups', () => {
    const entries = buildRunEventTimelineEntries([
      createEvent('1', 'tool_stream', { tool_call_id: 'tool-a', name: 'read_file', content: 'A' }),
      createEvent('2', 'tool_stream', { tool_call_id: 'tool-b', name: 'read_file', content: 'B' }),
    ]);

    expect(entries).toHaveLength(2);
    expect(entries[0]?.kind).toBe('group');
    expect(entries[1]?.kind).toBe('group');
    expect(entries[0]?.events).toHaveLength(1);
    expect(entries[1]?.events).toHaveLength(1);
    expect(entries[0]?.aggregatedText).toBe('A');
    expect(entries[1]?.aggregatedText).toBe('B');
  });
});
