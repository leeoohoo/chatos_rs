// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { ChatStoreShape } from '../../types';
import { applySessionRuntimeMetadata } from './sessionState';

describe('applySessionRuntimeMetadata', () => {
  it('synchronizes current project from updated runtime metadata for the active session', () => {
    const state = {
      sessions: [{
        id: 'session_1',
        title: 'session_1',
        userId: 'user_1',
        user_id: 'user_1',
        projectId: null,
        project_id: null,
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
        messageCount: 0,
        tokenUsage: 0,
        pinned: false,
        archived: false,
        status: 'active',
        tags: null,
        metadata: null,
      }],
      projects: [{
        id: 'project_3',
        name: 'Project 3',
        rootPath: '/tmp/project-3',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
      }],
      currentSession: {
        id: 'session_1',
        title: 'session_1',
        userId: 'user_1',
        user_id: 'user_1',
        projectId: null,
        project_id: null,
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
        messageCount: 0,
        tokenUsage: 0,
        pinned: false,
        archived: false,
        status: 'active',
        tags: null,
        metadata: null,
      },
      currentProjectId: null,
      currentProject: null,
    } as unknown as ChatStoreShape;

    applySessionRuntimeMetadata(state, 'session_1', {
      chat_runtime: {
        project_id: 'project_3',
      },
    });

    expect(state.currentSession?.metadata).toEqual({
      chat_runtime: {
        project_id: 'project_3',
      },
    });
    expect(state.currentSession?.projectId).toBe('project_3');
    expect(state.currentSession?.project_id).toBe('project_3');
    expect(state.sessions[0]?.projectId).toBe('project_3');
    expect(state.sessions[0]?.project_id).toBe('project_3');
    expect(state.currentProjectId).toBe('project_3');
    expect(state.currentProject?.id).toBe('project_3');
  });
});
