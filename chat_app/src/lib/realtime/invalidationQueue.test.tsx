// @vitest-environment jsdom

import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { useRealtimeInvalidationQueue } from './invalidationQueue';

const flushMicrotasks = async () => {
  await Promise.resolve();
};

describe('useRealtimeInvalidationQueue', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('coalesces same-tick invalidations before executing', async () => {
    vi.useFakeTimers();
    const onExecute = vi.fn();
    const { result } = renderHook(() => useRealtimeInvalidationQueue<string>({
      delayMs: 100,
      onExecute,
    }));

    act(() => {
      result.current.run('first');
      result.current.run('second');
      result.current.run('third');
    });

    expect(onExecute).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(0);
      await flushMicrotasks();
    });

    expect(onExecute).toHaveBeenCalledTimes(1);
    expect(onExecute).toHaveBeenCalledWith('third');
  });

  it('keeps only the latest invalidation while an execution is cooling down', async () => {
    vi.useFakeTimers();
    const onExecute = vi.fn();
    const { result } = renderHook(() => useRealtimeInvalidationQueue<string>({
      delayMs: 100,
      onExecute,
    }));

    act(() => {
      result.current.run('first');
    });
    await act(async () => {
      vi.advanceTimersByTime(0);
      await flushMicrotasks();
    });

    act(() => {
      result.current.run('second');
      result.current.run('third');
    });
    await act(async () => {
      vi.advanceTimersByTime(100);
      await flushMicrotasks();
    });

    expect(onExecute).toHaveBeenCalledTimes(2);
    expect(onExecute).toHaveBeenNthCalledWith(1, 'first');
    expect(onExecute).toHaveBeenNthCalledWith(2, 'third');
  });

  it('does not execute pending invalidations after unmount', () => {
    vi.useFakeTimers();
    const onExecute = vi.fn();
    const { result, unmount } = renderHook(() => useRealtimeInvalidationQueue<string>({
      delayMs: 100,
      onExecute,
    }));

    act(() => {
      result.current.run('late');
    });
    unmount();
    act(() => {
      vi.advanceTimersByTime(100);
    });

    expect(onExecute).not.toHaveBeenCalled();
  });
});
