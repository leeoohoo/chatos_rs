// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { MessageTaskRunnerGraphResponse } from '../../lib/api/client/types';
import { normalizeMessageTaskGraphEdgesForDisplay } from './MessageTaskGraphPanel';

describe('normalizeMessageTaskGraphEdgesForDisplay', () => {
  it('keeps multiple direct prerequisites parallel instead of serializing same-depth nodes', () => {
    const graph: MessageTaskRunnerGraphResponse = {
      root_task_ids: ['current'],
      source_session_id: 'session-1',
      source_turn_id: 'turn-1',
      source_user_message_id: 'user-1',
      nodes: [
        {
          depth: 0,
          is_root: true,
          is_current_message: true,
          task: {
            id: 'current',
            title: '当前任务',
            status: 'running',
            prerequisite_task_ids: ['prereq-a', 'prereq-b'],
          },
        },
        {
          depth: 1,
          is_root: false,
          is_current_message: false,
          task: {
            id: 'prereq-a',
            title: '前置 A',
            status: 'completed',
            prerequisite_task_ids: [],
          },
        },
        {
          depth: 1,
          is_root: false,
          is_current_message: false,
          task: {
            id: 'prereq-b',
            title: '前置 B',
            status: 'completed',
            prerequisite_task_ids: [],
          },
        },
      ],
      edges: [
        {
          id: 'prereq-a->prereq-b',
          source: 'prereq-a',
          target: 'prereq-b',
          kind: 'prerequisite',
        },
        {
          id: 'prereq-b->current',
          source: 'prereq-b',
          target: 'current',
          kind: 'prerequisite',
        },
      ],
    };

    expect(normalizeMessageTaskGraphEdgesForDisplay(graph)).toEqual([
      {
        id: 'prereq-a->current',
        source: 'prereq-a',
        target: 'current',
        kind: 'prerequisite',
      },
      {
        id: 'prereq-b->current',
        source: 'prereq-b',
        target: 'current',
        kind: 'prerequisite',
      },
    ]);
  });

  it('keeps declared serial prerequisite edges even when raw depths match', () => {
    const graph: MessageTaskRunnerGraphResponse = {
      root_task_ids: ['current'],
      source_session_id: 'session-1',
      source_turn_id: 'turn-1',
      source_user_message_id: 'user-1',
      nodes: [
        {
          depth: 0,
          is_root: true,
          is_current_message: true,
          task: {
            id: 'current',
            title: '当前任务',
            status: 'running',
            prerequisite_task_ids: ['prereq-b'],
          },
        },
        {
          depth: 1,
          is_root: false,
          is_current_message: false,
          task: {
            id: 'prereq-a',
            title: '前置 A',
            status: 'completed',
            prerequisite_task_ids: [],
          },
        },
        {
          depth: 1,
          is_root: false,
          is_current_message: false,
          task: {
            id: 'prereq-b',
            title: '前置 B',
            status: 'completed',
            prerequisite_task_ids: ['prereq-a'],
          },
        },
      ],
      edges: [],
    };

    expect(normalizeMessageTaskGraphEdgesForDisplay(graph)).toEqual([
      {
        id: 'prereq-b->current',
        source: 'prereq-b',
        target: 'current',
        kind: 'prerequisite',
      },
      {
        id: 'prereq-a->prereq-b',
        source: 'prereq-a',
        target: 'prereq-b',
        kind: 'prerequisite',
      },
    ]);
  });
});
