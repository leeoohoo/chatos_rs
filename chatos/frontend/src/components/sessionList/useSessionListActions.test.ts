// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom
import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useSessionListActions } from './useSessionListActions';

describe('useSessionListActions', () => {
  it('opens cloud project creation without a desktop bridge', () => {
    const setProjectModalOpen = vi.fn();
    const setProjectSourceMode = vi.fn();
    const { result } = renderHook(() => useSessionListActions({
      apiClient: { createLocalConnectorProject: vi.fn() } as any,
      contacts: [],
      currentSession: null,
      terminals: [],
      currentTerminal: null,
      remoteConnections: [],
      currentRemoteConnection: null,
      ensureSessionForContact: vi.fn(),
      selectSession: vi.fn(),
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
      setProjectModalOpen,
      setProjectSourceMode,
      setTerminalError: vi.fn(),
      setTerminalModalOpen: vi.fn(),
      setTerminalExecuting: vi.fn(),
      setKeyFilePickerOpen: vi.fn(),
      openRemoteModalBase: vi.fn(),
      createCloudProject: vi.fn(),
      createTerminal: vi.fn(),
      selectProject: vi.fn(),
      selectTerminal: vi.fn(),
      loadProjects: vi.fn(),
      projectSourceMode: 'server',
      localConnectorWorkspaces: [],
      selectedLocalConnectorWorkspaceId: '',
      selectRemoteConnection: vi.fn(),
      openRemoteSftp: vi.fn(),
      cloudProjectName: '',
      cloudProjectGitUrl: '',
      cloudProjectZipFile: null,
      allowLocalProjectCreation: false,
    }));

    result.current.openProjectModal();

    expect(setProjectSourceMode).toHaveBeenCalledWith('server');
    expect(setProjectModalOpen).toHaveBeenCalledWith(true);
  });

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
      setTerminalError: vi.fn(),
      setTerminalModalOpen: vi.fn(),
      setTerminalExecuting: vi.fn(),
      setKeyFilePickerOpen: vi.fn(),
      openRemoteModalBase: vi.fn(),
      createCloudProject: vi.fn(),
      createTerminal: vi.fn(),
      selectProject: vi.fn(),
      selectTerminal: vi.fn(),
      loadProjects: vi.fn(),
      projectSourceMode: 'server',
      localConnectorWorkspaces: [],
      selectedLocalConnectorWorkspaceId: '',
      selectRemoteConnection: vi.fn(),
      openRemoteSftp: vi.fn(),
      cloudProjectName: '',
      cloudProjectGitUrl: '',
      cloudProjectZipFile: null,
      allowLocalProjectCreation: true,
    }));

    await result.current.handleSelectSession('contact-placeholder:contact-1');

    expect(selectSession).toHaveBeenCalledWith('session-1', {
      skipBackgroundSync: true,
    });
  });

  it('creates terminals from the selected local connector workspace', async () => {
    const createTerminal = vi.fn().mockResolvedValue({ id: 'terminal-1' });
    const setTerminalModalOpen = vi.fn();
    const setTerminalExecuting = vi.fn();

    const { result } = renderHook(() => useSessionListActions({
      apiClient: {
        createLocalConnectorProject: vi.fn(),
        execLocalConnectorTerminalCommand: vi.fn(),
      } as any,
      contacts: [],
      currentSession: null,
      terminals: [],
      currentTerminal: null,
      remoteConnections: [],
      currentRemoteConnection: null,
      ensureSessionForContact: vi.fn(),
      selectSession: vi.fn(),
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
      setTerminalError: vi.fn(),
      setTerminalModalOpen,
      setTerminalExecuting,
      setKeyFilePickerOpen: vi.fn(),
      openRemoteModalBase: vi.fn(),
      createCloudProject: vi.fn(),
      createTerminal,
      selectProject: vi.fn(),
      selectTerminal: vi.fn(),
      loadProjects: vi.fn(),
      projectSourceMode: 'server',
      localConnectorWorkspaces: [{
        id: 'workspace-1',
        deviceId: 'device-1',
        label: 'MacBook / repo',
        alias: 'repo',
      }],
      selectedLocalConnectorWorkspaceId: 'workspace-1',
      selectedLocalConnectorDirectoryPath: 'apps/backend',
      selectRemoteConnection: vi.fn(),
      openRemoteSftp: vi.fn(),
      cloudProjectName: '',
      cloudProjectGitUrl: '',
      cloudProjectZipFile: null,
      allowLocalProjectCreation: true,
    }));

    await result.current.handleCreateTerminal();

    expect(createTerminal).toHaveBeenCalledWith(
      'local://connector/device-1/workspace-1/apps/backend',
      'backend',
    );
    expect(setTerminalModalOpen).toHaveBeenCalledWith(false);
    expect(setTerminalExecuting).toHaveBeenNthCalledWith(1, true);
    expect(setTerminalExecuting).toHaveBeenLastCalledWith(false);
  });

  it('opens a new local project without waiting for the cloud project refresh', async () => {
    const selectProject = vi.fn().mockResolvedValue(undefined);
    const loadProjects = vi.fn().mockReturnValue(new Promise(() => {}));
    const setProjectModalOpen = vi.fn();
    const { result } = renderHook(() => useSessionListActions({
      apiClient: {
        createLocalConnectorProject: vi.fn().mockResolvedValue({ id: 'local-project-1' }),
        execLocalConnectorTerminalCommand: vi.fn(),
      } as any,
      contacts: [],
      currentSession: null,
      terminals: [],
      currentTerminal: null,
      remoteConnections: [],
      currentRemoteConnection: null,
      ensureSessionForContact: vi.fn(),
      selectSession: vi.fn(),
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
      setProjectModalOpen,
      setProjectSourceMode: vi.fn(),
      setTerminalError: vi.fn(),
      setTerminalModalOpen: vi.fn(),
      setTerminalExecuting: vi.fn(),
      setKeyFilePickerOpen: vi.fn(),
      openRemoteModalBase: vi.fn(),
      createCloudProject: vi.fn(),
      createTerminal: vi.fn(),
      selectProject,
      selectTerminal: vi.fn(),
      loadProjects,
      projectSourceMode: 'local_connector',
      localConnectorWorkspaces: [{
        id: 'workspace-1',
        deviceId: 'device-1',
        label: 'MacBook / repo',
        alias: 'repo',
      }],
      selectedLocalConnectorWorkspaceId: 'workspace-1',
      selectedLocalConnectorDirectoryPath: 'apps/backend',
      selectRemoteConnection: vi.fn(),
      openRemoteSftp: vi.fn(),
      cloudProjectName: '',
      cloudProjectGitUrl: '',
      cloudProjectZipFile: null,
      allowLocalProjectCreation: true,
    }));

    await result.current.handleCreateProject();

    expect(selectProject).toHaveBeenCalledWith('local-project-1');
    expect(loadProjects).toHaveBeenCalledWith({ force: true });
    expect(setProjectModalOpen).toHaveBeenCalledWith(false);
  });
});
