import { act, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '../../../api';
import { renderHook } from '../../../test/renderHook';
import type { EngineSource } from '../../../types';
import { useCatalogLoaders } from './useCatalogLoaders';

vi.mock('../../../api', () => ({
  api: {
    listSources: vi.fn(),
    listModelProfiles: vi.fn(),
    listJobPolicies: vi.fn(),
  },
}));

describe('useCatalogLoaders', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('ignores stale source list responses when a newer refresh finishes first', async () => {
    const listSources = vi.mocked(api.listSources);
    const setSources = vi.fn();
    const setSourcesLoading = vi.fn();

    let resolveFirst: ((value: EngineSource[]) => void) | null = null;
    let resolveSecond: ((value: EngineSource[]) => void) | null = null;

    listSources
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

    const { result } = renderHook(() =>
      useCatalogLoaders({
        setSources,
        setModelProfiles: vi.fn(),
        setJobPolicies: vi.fn(),
        setSourcesLoading,
        setModelsLoading: vi.fn(),
        setPoliciesLoading: vi.fn(),
      }),
    );

    act(() => {
      void result.current.loadSources();
    });

    await act(async () => {
      const secondPromise = result.current.loadSources();
      resolveSecond?.([
        {
          id: 'new',
          tenant_id: 'tenant-a',
          source_id: 'source-new',
          name: 'new-source',
          source_type: 'sdk',
          status: 'active',
          sdk_enabled: true,
          created_at: '2026-05-20T00:00:00Z',
          updated_at: '2026-05-20T00:00:00Z',
        },
      ]);
      await secondPromise;
    });

    await waitFor(() => {
      expect(setSources).toHaveBeenCalledWith([
        {
          id: 'new',
          tenant_id: 'tenant-a',
          source_id: 'source-new',
          name: 'new-source',
          source_type: 'sdk',
          status: 'active',
          sdk_enabled: true,
          created_at: '2026-05-20T00:00:00Z',
          updated_at: '2026-05-20T00:00:00Z',
        },
      ]);
    });

    await act(async () => {
      resolveFirst?.([
        {
          id: 'old',
          tenant_id: 'tenant-a',
          source_id: 'source-old',
          name: 'old-source',
          source_type: 'sdk',
          status: 'active',
          sdk_enabled: true,
          created_at: '2026-05-20T00:00:00Z',
          updated_at: '2026-05-20T00:00:00Z',
        },
      ]);
      await Promise.resolve();
    });

    expect(setSources).toHaveBeenCalledTimes(1);
    expect(setSourcesLoading).toHaveBeenLastCalledWith(false);
  });
});
