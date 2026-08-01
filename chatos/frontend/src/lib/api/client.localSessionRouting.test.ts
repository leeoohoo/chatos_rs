// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

import ApiClient from './client';

describe('desktop session execution-plane routing', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('keeps every business session on the cloud runtime', () => {
    vi.stubGlobal('window', {
      chatosLocalRuntime: {
        apiRequest: vi.fn(),
      },
    });
    const client = new ApiClient('/api');
    client.registerProjectExecution({
      id: 'cloud-project',
      executionPlane: 'cloud',
      sourceType: 'cloud',
      rootPath: '',
    } as never);

    expect(client.sessionScopeUsesLocalRuntime('-1')).toBe(false);
    expect(client.sessionScopeUsesLocalRuntime(null)).toBe(false);
    expect(client.sessionScopeUsesLocalRuntime('local-project-not-yet-cached')).toBe(false);
    expect(client.sessionScopeUsesLocalRuntime('cloud-project')).toBe(false);
  });

  it('does not expose the local runtime to an ordinary browser surface', () => {
    vi.stubGlobal('window', {});
    const client = new ApiClient('/api');
    expect(client.sessionScopeUsesLocalRuntime('-1')).toBe(false);
  });
});
