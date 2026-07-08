// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import {
  applyTerminalHistoryLoadSuccess,
  beginTerminalHistoryLoad,
  finalizeTerminalHistoryLoad,
  handleTerminalHistoryLoadError,
  resolveTerminalHistoryRequest,
} from './terminalHistoryEffects';

const createRef = <T,>(current: T) => ({ current });

describe('terminalHistoryEffects', () => {
  it('marks load-more requests as busy and clears stale errors before loading', () => {
    const setHistoryBusy = vi.fn();
    const setErrorMessage = vi.fn();

    beginTerminalHistoryLoad({
      mode: 'more',
      setHistoryBusy,
      setErrorMessage,
    });

    expect(setHistoryBusy).toHaveBeenCalledWith(true);
    expect(setErrorMessage).toHaveBeenCalledWith(null);
  });

  it('short-circuits load-more requests when the cursor is missing', () => {
    const setCanLoadMoreHistory = vi.fn();
    const setHistoryBusy = vi.fn();

    const resolved = resolveTerminalHistoryRequest({
      mode: 'more',
      limit: 9999,
      historyBeforeCursorRef: createRef<string | null>(null),
      setCanLoadMoreHistory,
      setHistoryBusy,
    });

    expect(resolved).toEqual({
      shouldStop: true,
      requestLimit: 3000,
      requestBefore: null,
    });
    expect(setCanLoadMoreHistory).toHaveBeenCalledWith(false);
    expect(setHistoryBusy).toHaveBeenCalledWith(false);
  });

  it('replays initial output, merges history, and emits readiness state after a successful load', async () => {
    const reset = vi.fn();
    const write = vi.fn((_chunk: string, callback?: () => void) => callback?.());
    const term = {
      reset,
      write,
    } as unknown as import('@xterm/xterm').Terminal;
    const inputParseStateRef = createRef({
      lineBuffer: 'stale',
      skipFollowingLf: true,
    });
    const outputParseStateRef = createRef({
      lineBuffer: 'stale-output',
    });
    const commandSeqRef = createRef(4);
    const historyLoadedCountRef = createRef(0);
    const historyLoadedIdsRef = createRef(new Set<string>());
    const historyBeforeCursorRef = createRef<string | null>(null);
    const replayingHistoryRef = createRef(true);
    const commandHistoryCacheRef = createRef<Record<string, Array<{
      id: string;
      command: string;
      createdAt: string;
    }>>>({});
    const terminalOpenStartedAtRef = createRef(Date.now() - 50);

    const setHistoryState = vi.fn();
    const setCommandHistory = vi.fn();
    const setHistoryLogLimit = vi.fn();
    const setCanLoadMoreHistory = vi.fn();
    const setHistoryModeHint = vi.fn();

    await applyTerminalHistoryLoadSuccess({
      terminalId: 'terminal_1',
      term,
      mode: 'initial',
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
      ],
      requestLimit: 120,
      inputParseStateRef,
      outputParseStateRef,
      commandSeqRef,
      historyLoadedCountRef,
      historyLoadedIdsRef,
      historyBeforeCursorRef,
      replayingHistoryRef,
      commandHistoryCacheRef,
      terminalOpenStartedAtRef,
      setHistoryState,
      setCommandHistory,
      setHistoryLogLimit,
      setCanLoadMoreHistory,
      setHistoryModeHint,
    });

    expect(reset).toHaveBeenCalledTimes(1);
    expect(write).toHaveBeenCalledWith('On branch main\n', expect.any(Function));
    expect(inputParseStateRef.current).toEqual({
      lineBuffer: '',
      skipFollowingLf: false,
    });
    expect(outputParseStateRef.current).toEqual({
      lineBuffer: '',
    });
    expect(commandSeqRef.current).toBe(5);
    expect(historyLoadedCountRef.current).toBe(2);
    expect(Array.from(historyLoadedIdsRef.current)).toEqual(['log_1', 'log_2']);
    expect(historyBeforeCursorRef.current).toBe('2026-05-20T10:00:00.000Z');
    expect(replayingHistoryRef.current).toBe(false);
    expect(setCanLoadMoreHistory).toHaveBeenCalledWith(false);
    expect(setCommandHistory).toHaveBeenCalledWith([
      {
        id: 'cmd-4',
        command: 'git status',
        createdAt: '2026-05-20T10:00:00.000Z',
      },
    ]);
    expect(commandHistoryCacheRef.current.terminal_1).toEqual([
      {
        id: 'cmd-4',
        command: 'git status',
        createdAt: '2026-05-20T10:00:00.000Z',
      },
    ]);
    expect(setHistoryLogLimit).toHaveBeenCalledWith(2);
    expect(setHistoryModeHint).toHaveBeenCalledWith(null);
    expect(setHistoryState).toHaveBeenCalledWith('ready');
  });

  it('keeps load-more failures local while surfacing an error message', () => {
    const setHistoryState = vi.fn();
    const setCanLoadMoreHistory = vi.fn();
    const setErrorMessage = vi.fn();

    const handled = handleTerminalHistoryLoadError({
      error: new Error('boom'),
      mode: 'more',
      isCancelled: () => false,
      isCurrentRequest: () => true,
      setHistoryState,
      setCanLoadMoreHistory,
      setErrorMessage,
    });

    expect(handled).toBe(true);
    expect(setHistoryState).not.toHaveBeenCalled();
    expect(setCanLoadMoreHistory).not.toHaveBeenCalled();
    expect(setErrorMessage).toHaveBeenCalledWith('boom');
  });

  it('finalizes active requests by clearing replay buffers and busy state', () => {
    const replayingHistoryRef = createRef(true);
    const pendingOutputChunksRef = createRef(['chunk']);
    const setHistoryBusy = vi.fn();

    const finalized = finalizeTerminalHistoryLoad({
      isCancelled: () => false,
      isCurrentRequest: () => true,
      replayingHistoryRef,
      pendingOutputChunksRef,
      setHistoryBusy,
    });

    expect(finalized).toBe(true);
    expect(replayingHistoryRef.current).toBe(false);
    expect(pendingOutputChunksRef.current).toEqual([]);
    expect(setHistoryBusy).toHaveBeenCalledWith(false);
  });
});
