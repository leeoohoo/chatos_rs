// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import { renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useSessionListBootstrap } from './useSessionListBootstrap';

describe('useSessionListBootstrap', () => {
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
