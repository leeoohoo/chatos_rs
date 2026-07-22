// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
// @vitest-environment jsdom

import { cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Session } from '../../types';
import { useContactProjectScope } from './useContactProjectScope';

afterEach(() => {
  cleanup();
});

describe('useContactProjectScope', () => {
  it('falls back to the concrete session project when the composer has no selection', async () => {
    const getContactProjects = vi.fn().mockResolvedValue([
      { project_id: 'project-session' },
    ]);
    const currentSession = {
      id: 'session-project-scope',
      projectId: 'project-session',
    } as Session;

    const { result } = renderHook(() => useContactProjectScope({
      apiClient: { getContactProjects },
      currentSession,
      currentContactId: 'contact-project-scope',
      projects: [{ id: 'project-session', name: 'Session project' }],
    }));

    expect(result.current.currentProjectIdForMemory).toBe('project-session');
    expect(result.current.currentProjectNameForMemory).toBe('Session project');

    await waitFor(() => {
      expect(result.current.composerProjectId).toBe('project-session');
    });
  });
});
