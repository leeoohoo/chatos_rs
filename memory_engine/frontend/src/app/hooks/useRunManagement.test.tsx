import { act, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '../../api';
import { renderHook } from '../../test/renderHook';
import { useRunManagement } from './useRunManagement';

vi.mock('../../api', () => ({
  api: {
    getJobRunsBundle: vi.fn(),
  },
}));

describe('useRunManagement', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('loads runs from the bundle endpoint and applies the snapshot', async () => {
    const getJobRunsBundle = vi.mocked(api.getJobRunsBundle);
    getJobRunsBundle.mockResolvedValue({
      thread_runs: [
        {
          id: 'run-thread-1',
          job_type: 'summary',
          trigger_type: 'thread_direct',
          status: 'done',
          input_count: 1,
          output_count: 1,
          processed_count: 1,
          success_count: 1,
          error_count: 0,
          started_at: '2026-05-20T00:00:00Z',
        },
      ],
      scheduler_runs: [
        {
          id: 'run-scheduler-1',
          job_type: 'rollup',
          trigger_type: 'scheduler',
          status: 'running',
          input_count: 2,
          output_count: 0,
          processed_count: 1,
          success_count: 0,
          error_count: 0,
          started_at: '2026-05-20T00:00:00Z',
        },
      ],
    });

    const { result } = renderHook(() => useRunManagement());

    await act(async () => {
      await result.current.loadRuns({
        tenant_id: 'tenant-a',
        source_id: 'source-a',
        trigger_type: 'scheduler',
        thread_id: 'thread-123',
        limit: 50,
      });
    });

    expect(getJobRunsBundle).toHaveBeenCalledTimes(1);
    expect(getJobRunsBundle).toHaveBeenCalledWith({
      tenant_id: 'tenant-a',
      source_id: 'source-a',
      trigger_type: 'scheduler',
      thread_id: 'thread-123',
      limit: 50,
    });
    expect(result.current.threadJobRuns).toHaveLength(1);
    expect(result.current.schedulerJobRuns).toHaveLength(1);
    await waitFor(() => {
      expect(result.current.runsLoading).toBe(false);
    });
  });

  it('keeps subject_direct runs in the direct bucket snapshot', async () => {
    const getJobRunsBundle = vi.mocked(api.getJobRunsBundle);
    getJobRunsBundle.mockResolvedValue({
      thread_runs: [
        {
          id: 'run-subject-1',
          job_type: 'subject_memory',
          trigger_type: 'subject_direct',
          subject_id: 'agent:abc',
          status: 'done',
          input_count: 12,
          output_count: 1,
          processed_count: 12,
          success_count: 1,
          error_count: 0,
          started_at: '2026-05-20T00:00:00Z',
        },
      ],
      scheduler_runs: [],
    });

    const { result } = renderHook(() => useRunManagement());

    await act(async () => {
      await result.current.loadRuns({
        job_type: 'subject_memory',
        limit: 50,
      });
    });

    expect(result.current.threadJobRuns).toHaveLength(1);
    expect(result.current.threadJobRuns[0]?.trigger_type).toBe('subject_direct');
    expect(result.current.schedulerJobRuns).toHaveLength(0);
  });

  it('keeps scheduler-derived summary and rollup runs in the scheduler bucket snapshot', async () => {
    const getJobRunsBundle = vi.mocked(api.getJobRunsBundle);
    getJobRunsBundle.mockResolvedValue({
      thread_runs: [],
      scheduler_runs: [
        {
          id: 'run-summary-worker-1',
          job_type: 'summary',
          trigger_type: 'scheduler',
          thread_id: 'thread-a',
          status: 'done',
          input_count: 3,
          output_count: 1,
          processed_count: 3,
          success_count: 1,
          error_count: 0,
          started_at: '2026-05-20T00:00:00Z',
        },
        {
          id: 'run-rollup-worker-1',
          job_type: 'rollup',
          trigger_type: 'scheduler',
          thread_id: 'thread-a',
          status: 'done',
          input_count: 5,
          output_count: 1,
          processed_count: 5,
          success_count: 1,
          error_count: 0,
          started_at: '2026-05-20T00:01:00Z',
        },
      ],
    });

    const { result } = renderHook(() => useRunManagement());

    await act(async () => {
      await result.current.loadRuns({
        limit: 50,
      });
    });

    expect(result.current.threadJobRuns).toHaveLength(0);
    expect(result.current.schedulerJobRuns).toHaveLength(2);
    expect(result.current.schedulerJobRuns.every((item) => item.trigger_type === 'scheduler')).toBe(true);
  });

  it('ignores stale run bundle responses when a newer refresh finishes first', async () => {
    const getJobRunsBundle = vi.mocked(api.getJobRunsBundle);

    let resolveFirst: ((value: {
      thread_runs: Array<{
        id: string;
        job_type: string;
        trigger_type: string;
        status: string;
        input_count: number;
        output_count: number;
        processed_count: number;
        success_count: number;
        error_count: number;
        started_at: string;
      }>;
      scheduler_runs: Array<{
        id: string;
        job_type: string;
        trigger_type: string;
        status: string;
        input_count: number;
        output_count: number;
        processed_count: number;
        success_count: number;
        error_count: number;
        started_at: string;
      }>;
    }) => void) | null = null;
    let resolveSecond: ((value: {
      thread_runs: Array<{
        id: string;
        job_type: string;
        trigger_type: string;
        status: string;
        input_count: number;
        output_count: number;
        processed_count: number;
        success_count: number;
        error_count: number;
        started_at: string;
      }>;
      scheduler_runs: Array<{
        id: string;
        job_type: string;
        trigger_type: string;
        status: string;
        input_count: number;
        output_count: number;
        processed_count: number;
        success_count: number;
        error_count: number;
        started_at: string;
      }>;
    }) => void) | null = null;

    getJobRunsBundle
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirst = resolve;
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveSecond = resolve;
          }),
      );

    const { result } = renderHook(() => useRunManagement());

    act(() => {
      void result.current.loadRuns({
        tenant_id: 'tenant-a',
        limit: 20,
      });
    });

    await act(async () => {
      const secondPromise = result.current.loadRuns({
        tenant_id: 'tenant-a',
        source_id: 'source-b',
        limit: 10,
      });
      resolveSecond?.({
        thread_runs: [
          {
            id: 'run-thread-new',
            job_type: 'summary',
            trigger_type: 'thread_direct',
            status: 'running',
            input_count: 3,
            output_count: 0,
            processed_count: 1,
            success_count: 0,
            error_count: 0,
            started_at: '2026-05-20T00:00:00Z',
          },
        ],
        scheduler_runs: [],
      });
      await secondPromise;
    });

    await waitFor(() => {
      expect(result.current.threadJobRuns[0]?.id).toBe('run-thread-new');
      expect(result.current.runsLoading).toBe(false);
    });

    await act(async () => {
      resolveFirst?.({
        thread_runs: [
          {
            id: 'run-thread-old',
            job_type: 'rollup',
            trigger_type: 'thread_direct',
            status: 'done',
            input_count: 1,
            output_count: 1,
            processed_count: 1,
            success_count: 1,
            error_count: 0,
            started_at: '2026-05-20T00:00:00Z',
          },
        ],
        scheduler_runs: [],
      });
      await Promise.resolve();
    });

    expect(result.current.threadJobRuns[0]?.id).toBe('run-thread-new');
    expect(result.current.schedulerJobRuns).toHaveLength(0);
  });

  it('reports bundle load failures without leaving loading stuck', async () => {
    const getJobRunsBundle = vi.mocked(api.getJobRunsBundle);
    const onError = vi.fn();

    getJobRunsBundle.mockRejectedValue(new Error('network down'));

    const { result } = renderHook(() => useRunManagement({ onError }));

    await act(async () => {
      await result.current.loadRuns({
        tenant_id: 'tenant-a',
        limit: 20,
      });
    });

    expect(onError).toHaveBeenCalledWith('加载任务运行失败：Error: network down');
    await waitFor(() => {
      expect(result.current.runsLoading).toBe(false);
    });
  });
});
