// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from 'vitest';

import { ApiRequestError } from '../client/shared';
import { requestLocalRuntime } from './bridge';

afterEach(() => {
  delete window.chatosLocalRuntime;
  vi.useRealTimers();
});

describe('requestLocalRuntime authentication readiness', () => {
  it('retries the exact not-authenticated readiness response', async () => {
    vi.useFakeTimers();
    const apiRequest = vi.fn()
      .mockResolvedValueOnce({
        status: 409,
        ok: false,
        body: JSON.stringify({
          code: 'local_runtime_not_authenticated',
          message: 'Local Connector must be logged in before using the local runtime',
        }),
      })
      .mockResolvedValueOnce({
        status: 200,
        ok: true,
        body: JSON.stringify({ items: ['ready'] }),
      });
    window.chatosLocalRuntime = { apiRequest };

    const responsePromise = requestLocalRuntime<{ items: string[] }>(
      '/api/local/runtime/projects',
    );
    await vi.advanceTimersByTimeAsync(100);

    await expect(responsePromise).resolves.toEqual({ items: ['ready'] });
    expect(apiRequest).toHaveBeenCalledTimes(2);
  });

  it('does not retry unrelated conflict responses', async () => {
    const apiRequest = vi.fn().mockResolvedValue({
      status: 409,
      ok: false,
      body: JSON.stringify({
        code: 'workspace_conflict',
        message: 'workspace conflict',
      }),
    });
    window.chatosLocalRuntime = { apiRequest };

    await expect(requestLocalRuntime('/api/local/runtime/projects')).rejects.toMatchObject({
      status: 409,
      code: 'workspace_conflict',
    } satisfies Partial<ApiRequestError>);
    expect(apiRequest).toHaveBeenCalledTimes(1);
  });
});
