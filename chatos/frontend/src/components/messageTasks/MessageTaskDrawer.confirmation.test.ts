// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { Message } from '../../types';
import type { MessageTaskRunnerGraphResponse } from '../../lib/api/client/types';
import { resolveProjectExecutionConfirmationState } from './MessageTaskDrawer';

const message = (overallStatus: string): Message => ({
  id: 'message-1',
  sessionId: 'session-1',
  role: 'user',
  content: 'Generate a plan',
  status: 'completed',
  createdAt: new Date('2026-07-22T00:00:00Z'),
  metadata: {
    conversation_turn_id: 'execution-group-1',
    project_requirement_execution: {
      project_id: 'project-1',
      requirement_id: 'requirement-1',
      contact_id: 'contact-1',
    },
    task_runner_async: {
      mode: 'project_requirement_execution',
      overall_status: overallStatus,
      confirmation_status: overallStatus,
    },
  },
});

const graph = (status: string, lastRunId: string | null): MessageTaskRunnerGraphResponse => ({
  root_task_ids: ['task-1'],
  nodes: [{
    depth: 0,
    is_root: true,
    is_current_message: true,
    task: {
      id: 'task-1',
      title: 'Task 1',
      status,
      last_run_id: lastRunId,
    },
  }],
  edges: [],
  source_turn_id: 'execution-group-1',
  source_user_message_id: 'message-1',
});

describe('project execution confirmation state', () => {
  it('allows confirmation only for a complete deferred graph', () => {
    const taskGraph = graph('ready', null);
    const state = resolveProjectExecutionConfirmationState({
      graph: taskGraph,
      message: message('awaiting_confirmation'),
      tasks: taskGraph.nodes.map((node) => node.task),
    });

    expect(state.canConfirm).toBe(true);
    expect(state.executionGroupId).toBe('execution-group-1');
  });

  it('does not treat queued tasks as started when there is no run', () => {
    const queuedGraph = graph('queued', null);
    const queued = resolveProjectExecutionConfirmationState({
      graph: queuedGraph,
      message: message('confirmed'),
      tasks: queuedGraph.nodes.map((node) => node.task),
    });
    expect(queued.canConfirm).toBe(true);
    expect(queued.hasStartedTasks).toBe(false);

    const startedGraph = graph('running', 'run-1');
    const started = resolveProjectExecutionConfirmationState({
      graph: startedGraph,
      message: message('awaiting_confirmation'),
      tasks: startedGraph.nodes.map((node) => node.task),
    });
    expect(started.canConfirm).toBe(false);
    expect(started.hasStartedTasks).toBe(true);
  });
});
