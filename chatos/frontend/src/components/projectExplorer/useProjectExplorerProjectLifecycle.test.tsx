// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import { cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { useProjectExplorerProjectLifecycle } from './useProjectExplorerProjectLifecycle';

type LifecycleOptions = Parameters<typeof useProjectExplorerProjectLifecycle>[0];

const rootPath = 'local://connector/device/workspace/zj/ewo/vrad-backend';

const createBaseOptions = (
  overrides: Partial<LifecycleOptions> = {},
): LifecycleOptions => ({
  projectId: 'project_1',
  projectRootPath: rootPath,
  filesTabActive: true,
  toExpandedKey: (path) => path,
  keyToPath: (key) => key,
  loadEntries: vi.fn(async () => undefined),
  tryLoadEntries: vi.fn(async () => true),
  clearDragExpandTimer: vi.fn(),
  clearDragAutoScroll: vi.fn(),
  setEntriesMap: vi.fn(),
  setLoadingPaths: vi.fn(),
  setExpandedPaths: vi.fn(),
  setSelectedPath: vi.fn(),
  setSelectedFile: vi.fn(),
  setActionMessage: vi.fn(),
  setActionError: vi.fn(),
  setActionLoading: vi.fn(),
  setContextMenu: vi.fn(),
  setMoveConflict: vi.fn(),
  setDraggingEntryPath: vi.fn(),
  setDropTargetDirPath: vi.fn(),
  setExpandedReady: vi.fn(),
  ...overrides,
});

describe('useProjectExplorerProjectLifecycle', () => {
  afterEach(() => {
    cleanup();
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it('keeps the loaded tree when returning to the files tab for the same project', async () => {
    const setEntriesMap = vi.fn();
    const setLoadingPaths = vi.fn();
    const loadEntries = vi.fn(async () => undefined);
    const stableOptions = createBaseOptions({
      setEntriesMap,
      setLoadingPaths,
      loadEntries,
    });

    const { rerender } = renderHook(
      (options: LifecycleOptions) => useProjectExplorerProjectLifecycle(options),
      { initialProps: stableOptions },
    );

    await waitFor(() => {
      expect(loadEntries).toHaveBeenCalledWith(rootPath, { silent: false });
    });
    expect(setEntriesMap).toHaveBeenCalledTimes(1);
    expect(setEntriesMap).toHaveBeenCalledWith({});

    setEntriesMap.mockClear();
    setLoadingPaths.mockClear();
    loadEntries.mockClear();

    rerender({ ...stableOptions, filesTabActive: false });

    await waitFor(() => {
      expect(setLoadingPaths).toHaveBeenCalledTimes(1);
    });
    expect((setLoadingPaths.mock.calls[0]?.[0] as Set<string>).size).toBe(0);
    expect(setEntriesMap).not.toHaveBeenCalled();
    expect(loadEntries).not.toHaveBeenCalled();

    setLoadingPaths.mockClear();

    rerender({ ...stableOptions, filesTabActive: true });

    await waitFor(() => {
      expect(loadEntries).toHaveBeenCalledWith(rootPath, { silent: true });
    });
    expect(setEntriesMap).not.toHaveBeenCalled();
    expect((setLoadingPaths.mock.calls[0]?.[0] as Set<string>).size).toBe(0);
  });
});
