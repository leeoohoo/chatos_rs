// @vitest-environment jsdom

import { renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useSessionListBootstrap } from './useSessionListBootstrap';

describe('useSessionListBootstrap', () => {
  it('skips the initial terminal load when the terminal section is disabled', async () => {
    const loadTerminals = vi.fn();

    const { rerender } = renderHook(({ terminalsEnabled }) => useSessionListBootstrap({
      loadSessions: vi.fn(),
      loadProjects: vi.fn(),
      loadAgents: vi.fn(),
      loadContacts: vi.fn(),
      loadTerminals,
      loadRemoteConnections: vi.fn(),
      isCollapsed: false,
      terminalsEnabled,
      terminalsExpanded: false,
      remoteExpanded: false,
    }), {
      initialProps: {
        terminalsEnabled: false,
      },
    });

    await waitFor(() => {
      expect(loadTerminals).not.toHaveBeenCalled();
    });

    rerender({
      terminalsEnabled: true,
    });

    await waitFor(() => {
      expect(loadTerminals).toHaveBeenCalledTimes(1);
    });
  });
});
