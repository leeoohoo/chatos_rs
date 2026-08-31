// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { act, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '../../api';
import { renderHook } from '../../test/renderHook';
import { useConsoleResources } from './useConsoleResources';

const messageApi = {
  error: vi.fn(),
};

vi.mock('antd', async () => {
  const actual = await vi.importActual<typeof import('antd')>('antd');
  return {
    ...actual,
    App: {
      ...actual.App,
      useApp: () => ({ message: messageApi }),
    },
  };
});

vi.mock('../../api', () => ({
  api: {
    getDashboardOverview: vi.fn(),
  },
}));

describe('useConsoleResources', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('initializes dashboard with overview data', async () => {
    const getDashboardOverview = vi.mocked(api.getDashboardOverview);
    getDashboardOverview.mockResolvedValue({
      source_count: 5,
      model_count: 3,
      policy_count: 4,
      job_stats: {
        summary: { done: 2, running: 1 },
        rollup: { failed: 1 },
      },
    });

    const { result } = renderHook(() => useConsoleResources());

    await waitFor(() => {
      expect(result.current.initialized).toBe(true);
      expect(result.current.loading).toBe(false);
    });

    expect(getDashboardOverview).toHaveBeenCalledTimes(1);
    expect(result.current.dashboardStats).toEqual({
      sources: 5,
      models: 3,
      policies: 4,
      running: 1,
      done: 2,
      failed: 1,
    });
    expect(result.current.dashboardJobStats).toEqual({
      summary: { done: 2, running: 1 },
      rollup: { failed: 1 },
    });
  });

  it('refreshes dashboard stats on demand', async () => {
    const getDashboardOverview = vi.mocked(api.getDashboardOverview);
    getDashboardOverview
      .mockResolvedValueOnce({
        source_count: 1,
        model_count: 1,
        policy_count: 1,
        job_stats: {},
      })
      .mockResolvedValueOnce({
        source_count: 2,
        model_count: 4,
        policy_count: 1,
        job_stats: {
          summary: { running: 3, done: 5 },
        },
      });

    const { result } = renderHook(() => useConsoleResources());

    await waitFor(() => {
      expect(result.current.initialized).toBe(true);
      expect(result.current.loading).toBe(false);
    });

    await act(async () => {
      await result.current.loadDashboardOverview();
    });

    expect(getDashboardOverview).toHaveBeenCalledTimes(2);
    expect(result.current.initialized).toBe(true);
    expect(result.current.dashboardStats).toEqual({
      sources: 2,
      models: 4,
      policies: 1,
      running: 3,
      done: 5,
      failed: 0,
    });
  });

  it('ignores stale dashboard refresh responses', async () => {
    const getDashboardOverview = vi.mocked(api.getDashboardOverview);

    let resolveInitial: ((value: {
      source_count: number;
      model_count: number;
      policy_count: number;
      job_stats: Record<string, Record<string, number>>;
    }) => void) | null = null;
    let resolveRefresh: ((value: {
      source_count: number;
      model_count: number;
      policy_count: number;
      job_stats: Record<string, Record<string, number>>;
    }) => void) | null = null;

    getDashboardOverview
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveInitial = resolve;
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveRefresh = resolve;
          }),
      );

    const { result } = renderHook(() => useConsoleResources());

    await waitFor(() => {
      expect(result.current.loading).toBe(true);
    });

    await act(async () => {
      const refreshPromise = result.current.loadDashboardOverview();
      resolveRefresh?.({
        source_count: 9,
        model_count: 6,
        policy_count: 5,
        job_stats: {
          summary: { running: 2, done: 4 },
        },
      });
      await refreshPromise;
    });

    await waitFor(() => {
      expect(result.current.dashboardStats).toEqual({
        sources: 9,
        models: 6,
        policies: 5,
        running: 2,
        done: 4,
        failed: 0,
      });
    });

    await act(async () => {
      resolveInitial?.({
        source_count: 1,
        model_count: 1,
        policy_count: 1,
        job_stats: {
          summary: { running: 1 },
        },
      });
      await Promise.resolve();
    });

    expect(result.current.dashboardStats).toEqual({
      sources: 9,
      models: 6,
      policies: 5,
      running: 2,
      done: 4,
      failed: 0,
    });
  });
});
