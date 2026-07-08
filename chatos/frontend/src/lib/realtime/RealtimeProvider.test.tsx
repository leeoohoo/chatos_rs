// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import React from 'react';
import { act, render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import ApiClient from '../api/client';
import { ApiClientProvider } from '../api/ApiClientContext';
import {
  RealtimeProvider,
  useRealtimeConnectionState,
  useRealtimeDebugSnapshot,
  useRealtimeTopic,
} from './RealtimeProvider';

describe('RealtimeProvider', () => {
  it('does not rerender connection-state consumers for debug-only updates', async () => {
    const apiClient = new ApiClient('http://127.0.0.1:3997/api');
    let connectionRenderCount = 0;
    let debugRenderCount = 0;
    let setTopicEnabled: React.Dispatch<React.SetStateAction<boolean>> | null = null;

    const ConnectionProbe = () => {
      useRealtimeConnectionState();
      connectionRenderCount += 1;
      return null;
    };

    const DebugProbe = () => {
      useRealtimeDebugSnapshot();
      debugRenderCount += 1;
      return null;
    };

    const TopicSwitcher = () => {
      const [enabled, setEnabled] = React.useState(false);
      React.useEffect(() => {
        setTopicEnabled = setEnabled;
        return () => {
          setTopicEnabled = null;
        };
      }, []);
      useRealtimeTopic({ scope: 'project', id: 'project-1' }, enabled);
      return null;
    };

    render(
      <ApiClientProvider client={apiClient}>
        <RealtimeProvider>
          <ConnectionProbe />
          <DebugProbe />
          <TopicSwitcher />
        </RealtimeProvider>
      </ApiClientProvider>,
    );

    expect(connectionRenderCount).toBe(1);
    const debugRenderCountAfterMount = debugRenderCount;

    await act(async () => {
      setTopicEnabled?.(true);
    });

    expect(connectionRenderCount).toBe(1);
    expect(debugRenderCount).toBeGreaterThan(debugRenderCountAfterMount);
  });
});
