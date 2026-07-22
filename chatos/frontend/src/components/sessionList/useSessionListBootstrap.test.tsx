// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import { act, renderHook, waitFor } from '@testing-library/react';
import { StrictMode, type ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { useSessionListBootstrap } from './useSessionListBootstrap';

describe('useSessionListBootstrap', () => {
  it('completes the one-time bootstrap under React StrictMode', async () => {
    const loadProjects = vi.fn();
    const loadContacts = vi.fn();
    const loadSessions = vi.fn();

    renderHook(() => useSessionListBootstrap({
      loadSessions,
      loadProjects,
      loadAgents: vi.fn(),
      loadContacts,
      loadTerminals: vi.fn(),
      loadRemoteConnections: vi.fn(),
      isCollapsed: false,
      terminalsEnabled: false,
      remoteEnabled: false,
      terminalsExpanded: false,
      remoteExpanded: false,
    }), {
      wrapper: ({ children }: { children: ReactNode }) => (
        <StrictMode>{children}</StrictMode>
      ),
    });

    await waitFor(() => {
      expect(loadProjects).toHaveBeenCalledTimes(1);
      expect(loadContacts).toHaveBeenCalledTimes(1);
      expect(loadSessions).toHaveBeenCalledWith({ silent: true });
    });
  });

  it('restores the project before selecting the initial session', async () => {
    let resolveProjects: (() => void) | undefined;
    const loadProjects = vi.fn(() => new Promise<void>((resolve) => {
      resolveProjects = resolve;
    }));
    const loadSessions = vi.fn();

    renderHook(() => useSessionListBootstrap({
      loadSessions,
      loadProjects,
      loadAgents: vi.fn(),
      loadContacts: vi.fn(),
      loadTerminals: vi.fn(),
      loadRemoteConnections: vi.fn(),
      isCollapsed: false,
      terminalsEnabled: false,
      remoteEnabled: false,
      terminalsExpanded: false,
      remoteExpanded: false,
    }));

    await waitFor(() => {
      expect(loadProjects).toHaveBeenCalledTimes(1);
    });
    expect(loadProjects).toHaveBeenCalledWith({ force: true, throwOnError: true });
    expect(loadSessions).not.toHaveBeenCalled();

    await act(async () => {
      resolveProjects?.();
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(loadSessions).toHaveBeenCalledWith({ silent: true });
    });
  });

  it('retries transient project and contact bootstrap failures before loading sessions', async () => {
    const loadProjects = vi.fn()
      .mockRejectedValueOnce(new Error('user service unavailable'))
      .mockResolvedValue([]);
    const loadContacts = vi.fn()
      .mockRejectedValueOnce(new Error('user service unavailable'))
      .mockResolvedValue([]);
    const loadSessions = vi.fn();

    renderHook(() => useSessionListBootstrap({
      loadSessions,
      loadProjects,
      loadAgents: vi.fn(),
      loadContacts,
      loadTerminals: vi.fn(),
      loadRemoteConnections: vi.fn(),
      isCollapsed: false,
      terminalsEnabled: false,
      remoteEnabled: false,
      terminalsExpanded: false,
      remoteExpanded: false,
    }));

    await waitFor(() => {
      expect(loadProjects).toHaveBeenCalledTimes(2);
      expect(loadContacts).toHaveBeenCalledTimes(2);
      expect(loadSessions).toHaveBeenCalledWith({ silent: true });
    });
  });

  it('skips the initial terminal load when the terminal section is disabled', async () => {
    const loadTerminals = vi.fn();
    const loadRemoteConnections = vi.fn();

    const { rerender } = renderHook(({ terminalsEnabled, remoteEnabled }) => useSessionListBootstrap({
      loadSessions: vi.fn(),
      loadProjects: vi.fn(),
      loadAgents: vi.fn(),
      loadContacts: vi.fn(),
      loadTerminals,
      loadRemoteConnections,
      isCollapsed: false,
      terminalsEnabled,
      remoteEnabled,
      terminalsExpanded: false,
      remoteExpanded: false,
    }), {
      initialProps: {
        terminalsEnabled: false,
        remoteEnabled: false,
      },
    });

    await waitFor(() => {
      expect(loadTerminals).not.toHaveBeenCalled();
      expect(loadRemoteConnections).not.toHaveBeenCalled();
    });

    rerender({
      terminalsEnabled: true,
      remoteEnabled: true,
    });

    await waitFor(() => {
      expect(loadTerminals).toHaveBeenCalledTimes(1);
      expect(loadRemoteConnections).toHaveBeenCalledTimes(1);
    });
  });
});
