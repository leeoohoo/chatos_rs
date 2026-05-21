import { describe, expect, it } from 'vitest';

import {
  resolveHistoryRequestLimit,
  resolveTerminalHistoryLoad,
  resolveTerminalSnapshotLoadRequest,
  shouldSkipLoadMoreHistory,
} from './terminalHistoryState';

describe('terminalHistoryState', () => {
  it('bounds history request limit and short-circuits invalid load-more requests', () => {
    expect(resolveHistoryRequestLimit(0)).toBe(1);
    expect(resolveHistoryRequestLimit(999999)).toBe(3000);
    expect(shouldSkipLoadMoreHistory('more', null)).toBe(true);
    expect(shouldSkipLoadMoreHistory('initial', null)).toBe(false);
    expect(shouldSkipLoadMoreHistory('more', '2026-05-20T10:00:00.000Z')).toBe(false);
  });

  it('dedupes terminal logs, advances pagination, and merges command history', () => {
    const resolved = resolveTerminalHistoryLoad({
      logs: [
        {
          id: 'log_1',
          terminal_id: 'terminal_1',
          log_type: 'command',
          content: 'git status',
          created_at: '2026-05-20T10:00:00.000Z',
        },
        {
          id: 'log_2',
          terminal_id: 'terminal_1',
          log_type: 'output',
          content: 'On branch main\n',
          created_at: '2026-05-20T10:00:01.000Z',
        },
        {
          id: 'log_1',
          terminal_id: 'terminal_1',
          log_type: 'command',
          content: 'git status',
          created_at: '2026-05-20T10:00:00.000Z',
        },
      ],
      mode: 'more',
      requestLimit: 3,
      pagination: {
        loadedCount: 1,
        loadedIds: new Set(['older_log']),
        beforeCursor: '2026-05-20T09:59:00.000Z',
      },
      commandSequence: 5,
      cachedHistory: [
        {
          id: 'cached_1',
          command: 'pwd',
          createdAt: '2026-05-20T09:58:00.000Z',
        },
      ],
    });

    expect(resolved.uniqueLogs.map((log) => log.id)).toEqual(['log_1', 'log_2']);
    expect(resolved.nextPagination.loadedCount).toBe(3);
    expect(resolved.nextPagination.beforeCursor).toBe('2026-05-20T10:00:00.000Z');
    expect(Array.from(resolved.nextPagination.loadedIds)).toEqual(['older_log', 'log_1', 'log_2']);
    expect(resolved.canLoadMoreHistory).toBe(true);
    expect(resolved.mergedHistory.map((item) => item.command)).toEqual(['pwd', 'git status']);
    expect(resolved.nextHistoryModeHint).toContain('tail');
    expect(resolved.shouldReplayOutput).toBe(false);
    expect(resolved.outputReplay).toBe('On branch main\n');
  });

  it('computes snapshot scroll pagination requests conservatively', () => {
    expect(resolveTerminalSnapshotLoadRequest({
      viewportY: 3,
      terminalId: 'terminal_1',
      supportsSnapshotPaging: true,
      snapshotLoading: false,
      noMoreLines: false,
      currentLines: 500,
      scrollTopLoadThreshold: 0,
      initialLines: 500,
      maxLines: 10000,
      pageLines: 500,
      socketReadyState: 1,
    }).shouldRequest).toBe(false);

    const request = resolveTerminalSnapshotLoadRequest({
      viewportY: 0,
      terminalId: 'terminal_1',
      supportsSnapshotPaging: true,
      snapshotLoading: false,
      noMoreLines: false,
      currentLines: 500,
      scrollTopLoadThreshold: 0,
      initialLines: 500,
      maxLines: 10000,
      pageLines: 500,
      socketReadyState: 1,
    });
    expect(request.shouldRequest).toBe(true);
    expect(request.nextLines).toBe(1000);
    expect(request.requestContext).toEqual({
      terminalId: 'terminal_1',
      requestedLines: 1000,
      fromScroll: true,
    });

    const capped = resolveTerminalSnapshotLoadRequest({
      viewportY: 0,
      terminalId: 'terminal_1',
      supportsSnapshotPaging: true,
      snapshotLoading: false,
      noMoreLines: false,
      currentLines: 10000,
      scrollTopLoadThreshold: 0,
      initialLines: 500,
      maxLines: 10000,
      pageLines: 500,
      socketReadyState: 1,
    });
    expect(capped.shouldRequest).toBe(false);
    expect(capped.reachedEnd).toBe(true);
  });
});
