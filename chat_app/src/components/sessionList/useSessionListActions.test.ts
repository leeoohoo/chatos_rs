// @vitest-environment jsdom
import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useSessionListActions } from './useSessionListActions';

describe('useSessionListActions', () => {
  it('selects contact sessions with compact initial load settings', async () => {
    const selectSession = vi.fn().mockResolvedValue(undefined);
    const { result } = renderHook(() => useSessionListActions({
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
      setProjectError: vi.fn(),
      setProjectModalOpen: vi.fn(),
      setTerminalRoot: vi.fn(),
      setTerminalError: vi.fn(),
      setTerminalModalOpen: vi.fn(),
      setKeyFilePickerOpen: vi.fn(),
      openRemoteModalBase: vi.fn(),
      createProject: vi.fn(),
      createTerminal: vi.fn(),
      selectProject: vi.fn(),
      selectTerminal: vi.fn(),
      selectRemoteConnection: vi.fn(),
      openRemoteSftp: vi.fn(),
      projectRoot: '',
      terminalRoot: '',
    }));

    await result.current.handleSelectSession('contact-placeholder:contact-1');

    expect(selectSession).toHaveBeenCalledWith('session-1', {
      skipBackgroundSync: true,
    });
  });
});
