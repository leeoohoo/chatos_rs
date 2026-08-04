// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
// @vitest-environment jsdom

import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { useReviewRepairPolling } from './useReviewRepairPolling';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('useReviewRepairPolling', () => {
  it('polls cloud review status and reports completion', async () => {
    vi.useFakeTimers();
    const refreshStatus = vi.fn().mockResolvedValue({ running: false, pendingCount: 0 });
    const onCompleted = vi.fn();

    renderHook(() => useReviewRepairPolling({
      enabled: true,
      running: true,
      sessionId: 'cloud_session_memory',
      refreshStatus,
      onCompleted,
      onFailed: vi.fn(),
      fallbackErrorMessage: 'failed',
    }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(500);
    });

    expect(refreshStatus).toHaveBeenCalledWith('cloud_session_memory');
    expect(onCompleted).toHaveBeenCalledTimes(1);
  });

  it('does not poll when realtime fallback is disabled', async () => {
    vi.useFakeTimers();
    const refreshStatus = vi.fn();

    renderHook(() => useReviewRepairPolling({
      enabled: false,
      running: true,
      sessionId: 'cloud_session_1',
      refreshStatus,
      onCompleted: vi.fn(),
      onFailed: vi.fn(),
      fallbackErrorMessage: 'failed',
    }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_000);
    });

    expect(refreshStatus).not.toHaveBeenCalled();
  });
});
