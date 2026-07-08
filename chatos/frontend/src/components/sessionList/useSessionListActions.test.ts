// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom
import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useSessionListActions } from './useSessionListActions';

describe('useSessionListActions', () => {
  it('selects contact sessions with compact initial load settings', async () => {
    const selectSession = vi.fn().mockResolvedValue(undefined);
    const { result } = renderHook(() => useSessionListActions({
      apiClient: {
        createLocalConnectorProject: vi.fn(),
        execLocalConnectorTerminalCommand: vi.fn(),
      } as any,
      contacts: [{
        id: 'contact-1',
        agentId: 'agent-1',
        name: 'Alice',
        status: 'active',
        createdAt: new Date('2026-05-28T00:00:00.000Z'),
        updatedAt: new Date('2026-05-28T00:00:00.000Z'),
      }],
      currentSession: null,
      terminals: [],
      currentTerminal: null,
      remoteConnections: [],
      currentRemoteConnection: null,
      ensureSessionForContact: vi.fn().mockResolvedValue('session-1'),
      selectSession,
      setActivePanel: vi.fn(),
      loadContactsAction: vi.fn(),
      loadTerminals: vi.fn(),
      loadRemoteConnections: vi.fn(),
      setIsRefreshing: vi.fn(),
      setIsRefreshingTerminals: vi.fn(),
      setIsRefreshingRemote: vi.fn(),
      setProjectRoot: vi.fn(),
      setCloudProjectName: vi.fn(),
      setCloudProjectGitUrl: vi.fn(),
      setCloudProjectZipFile: vi.fn(),
      setProjectError: vi.fn(),
      setProjectModalOpen: vi.fn(),
      setProjectSourceMode: vi.fn(),
      setTerminalRoot: vi.fn(),
      setTerminalError: vi.fn(),
      setTerminalModalOpen: vi.fn(),
      setTerminalSourceMode: vi.fn(),
      setTerminalCommand: vi.fn(),
      setTerminalArgs: vi.fn(),
      setTerminalOutput: vi.fn(),
      setTerminalExecuting: vi.fn(),
      setKeyFilePickerOpen: vi.fn(),
      openRemoteModalBase: vi.fn(),
      createCloudProject: vi.fn(),
      createTerminal: vi.fn(),
      selectProject: vi.fn(),
      selectTerminal: vi.fn(),
      loadProjects: vi.fn(),
      projectSourceMode: 'server',
      terminalSourceMode: 'server',
      localConnectorWorkspaces: [],
      selectedLocalConnectorWorkspaceId: '',
      terminalCommand: 'pwd',
      terminalArgs: '',
      selectRemoteConnection: vi.fn(),
      openRemoteSftp: vi.fn(),
      cloudProjectName: '',
      cloudProjectGitUrl: '',
      cloudProjectZipFile: null,
      terminalRoot: '',
    }));

    await result.current.handleSelectSession('contact-placeholder:contact-1');

    expect(selectSession).toHaveBeenCalledWith('session-1', {
      skipBackgroundSync: true,
    });
  });
});
