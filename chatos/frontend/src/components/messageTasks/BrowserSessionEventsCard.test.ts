// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { browserSessionTargetsFromEvents } from './BrowserSessionEventsCard';

describe('browserSessionTargetsFromEvents', () => {
  it('keeps the latest routable managed session state', () => {
    const sessions = browserSessionTargetsFromEvents([
      {
        id: 'event-1',
        run_id: 'run-1',
        event_type: 'browser_session',
        payload: {
          id: 'h_session',
          mode: 'managed',
          status: 'active',
          workspace_id: 'workspace-1',
        },
      },
      {
        id: 'event-2',
        run_id: 'run-1',
        event_type: 'browser_session',
        payload: {
          id: 'h_session',
          mode: 'managed',
          status: 'error',
          workspace_id: 'workspace-1',
          device_id: 'device-1',
        },
      },
      {
        id: 'event-3',
        run_id: 'run-1',
        event_type: 'browser_session',
        payload: {
          id: 'cdp_session',
          mode: 'cdp',
          workspace_id: 'workspace-1',
        },
      },
    ]);

    expect(sessions).toEqual([{
      id: 'h_session',
      mode: 'managed',
      workspaceId: 'workspace-1',
      deviceId: 'device-1',
      projectId: null,
      status: 'error',
      url: null,
      title: null,
    }]);
  });
});
