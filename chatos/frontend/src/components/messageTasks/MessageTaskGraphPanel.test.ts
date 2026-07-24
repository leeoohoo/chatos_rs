// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { MessageTaskRunnerGraphResponse } from '../../lib/api/client/types';
import {
  normalizeMessageTaskGraphEdgesForDisplay,
  normalizeMessageTaskGraphForDisplay,
} from './MessageTaskGraphPanel';

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

  it('shows only the transitive reduction by default and preserves the full graph on demand', () => {
    const graph: MessageTaskRunnerGraphResponse = {
      root_task_ids: ['task-c'],
      nodes: [
        { depth: 2, is_root: false, is_current_message: true, task: { id: 'task-a', title: 'A', prerequisite_task_ids: [] } },
        { depth: 1, is_root: false, is_current_message: true, task: { id: 'task-b', title: 'B', prerequisite_task_ids: ['task-a'] } },
        { depth: 0, is_root: true, is_current_message: true, task: { id: 'task-c', title: 'C', prerequisite_task_ids: ['task-a', 'task-b'] } },
      ],
      edges: [],
    };

    expect(normalizeMessageTaskGraphForDisplay(graph).edges.map((edge) => edge.id)).toEqual([
      'task-a->task-b',
      'task-b->task-c',
    ]);
    expect(normalizeMessageTaskGraphForDisplay(graph, 'full').edges.map((edge) => edge.id)).toEqual([
      'task-a->task-b',
      'task-a->task-c',
      'task-b->task-c',
    ]);
  });

  it('renders non-blocking context only in the full graph', () => {
    const graph: MessageTaskRunnerGraphResponse = {
      root_task_ids: ['task-b'],
      nodes: [
        {
          depth: 1,
          is_root: false,
          is_current_message: true,
          task: {
            id: 'task-a',
            title: 'A',
            prerequisite_task_ids: [],
            input_payload: { execution_client_ref: 'a', dependency_context_refs: [] },
          },
        },
        {
          depth: 0,
          is_root: true,
          is_current_message: true,
          task: {
            id: 'task-b',
            title: 'B',
            prerequisite_task_ids: [],
            input_payload: { execution_client_ref: 'b', dependency_context_refs: ['a'] },
          },
        },
      ],
      edges: [],
    };

    expect(normalizeMessageTaskGraphForDisplay(graph).edges).toEqual([]);
    expect(normalizeMessageTaskGraphForDisplay(graph, 'full').edges).toEqual([
      { id: 'task-a->task-b', source: 'task-a', target: 'task-b', kind: 'context' },
    ]);
  });

  it('folds implementation and review stages bound to the same project task', () => {
    const graph: MessageTaskRunnerGraphResponse = {
      root_task_ids: ['review'],
      nodes: [
        {
          depth: 1,
          is_root: false,
          is_current_message: true,
          task: { id: 'implement', title: '实现订单校验', prerequisite_task_ids: [], input_payload: { project_task_id: 'project-task-1' } },
        },
        {
          depth: 0,
          is_root: true,
          is_current_message: true,
          task: { id: 'review', title: 'Review 订单校验', prerequisite_task_ids: ['implement'], input_payload: { project_task_id: 'project-task-1' } },
        },
      ],
      edges: [],
    };

    const display = normalizeMessageTaskGraphForDisplay(graph);
    expect(display.nodes).toHaveLength(1);
    expect(display.nodes[0].groupedTasks).toHaveLength(2);
    expect(display.edges).toEqual([]);
  });
});
