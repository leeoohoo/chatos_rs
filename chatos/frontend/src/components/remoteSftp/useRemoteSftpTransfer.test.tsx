// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
// @vitest-environment jsdom

import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { RemoteSftpClient } from './helpers';
import { useRemoteSftpTransfer } from './useRemoteSftpTransfer';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const transferResponse = {
  id: 'transfer-1',
  connection_id: 'connection-1',
  direction: 'upload' as const,
  state: 'running' as const,
  total_bytes: 100,
  transferred_bytes: 0,
  percent: 0,
  current_path: '/remote/file.txt',
  message: null,
  error: null,
  created_at: '2026-08-09T00:00:00Z',
  updated_at: '2026-08-09T00:00:00Z',
};

describe('useRemoteSftpTransfer', () => {
  it('keeps polling when a running transfer has no progress change', async () => {
    vi.useFakeTimers();
    const getRemoteSftpTransferStatus = vi.fn().mockResolvedValue(transferResponse);
    const client = {
      startRemoteSftpTransfer: vi.fn().mockResolvedValue(transferResponse),
      getRemoteSftpTransferStatus,
      cancelRemoteSftpTransfer: vi.fn(),
    } as unknown as RemoteSftpClient;
    const remotePathRef = { current: '/remote' };
    const localPathRef = { current: '/local' };
    const t = ((key: string) => key) as TranslateFn;
    const { result } = renderHook(() => useRemoteSftpTransfer({
      client,
      currentRemoteConnectionId: 'connection-1',
      loadLocal: vi.fn().mockResolvedValue(undefined),
      loadRemote: vi.fn().mockResolvedValue(undefined),
      remotePathRef,
      localPathRef,
      setMessage: vi.fn(),
      setError: vi.fn(),
      getVerificationCode: () => null,
      t,
      onSecondFactorRequired: () => false,
    }));

    act(() => {
      result.current.enqueueTransfer({
        direction: 'upload',
        localSource: '/local/file.txt',
        remoteSource: '/remote/file.txt',
        fallbackSuccess: 'uploaded',
        label: 'file.txt',
      });
    });
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_100);
    });

    expect(getRemoteSftpTransferStatus.mock.calls.length).toBeGreaterThanOrEqual(2);
  });
});
