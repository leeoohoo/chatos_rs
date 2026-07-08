// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import {
  cleanupTerminalInstanceSessionState,
  resetTerminalInstanceSessionState,
} from './terminalInstanceState';

const createRef = <T,>(current: T) => ({ current });

describe('terminalInstanceState', () => {
  it('resets terminal session state to the cached baseline for the active terminal', () => {
    const inputParseStateRef = createRef({ lineBuffer: 'stale', skipFollowingLf: true });
    const outputParseStateRef = createRef({ lineBuffer: 'stale-output' });
    const historyLoadedCountRef = createRef(12);
    const historyLoadedIdsRef = createRef(new Set(['log_old']));
    const historyBeforeCursorRef = createRef<string | null>('before_cursor');
    const replayingHistoryRef = createRef(true);
    const pendingOutputChunksRef = createRef(['stale chunk']);
    const commandHistoryCacheRef = createRef({
      terminal_1: [
        {
          id: 'cached_1',
          command: 'pwd',
          createdAt: '2026-05-20T10:00:00.000Z',
        },
      ],
    });
    const terminalOpenStartedAtRef = createRef<number | null>(null);
    const terminalFirstOutputLoggedRef = createRef(true);
    const snapshotVisibleLinesRef = createRef<Record<string, number>>({});
    const snapshotNoMoreLinesRef = createRef<Record<string, boolean>>({});
    const snapshotLoadingRef = createRef(true);
    const supportsSnapshotPagingRef = createRef(true);
    const snapshotRequestContextRef = createRef({
      terminalId: 'terminal_1',
      requestedLines: 1000,
      fromScroll: true,
    });
    const inputForwardEnabledRef = createRef(true);

    let commandHistoryValue: unknown = null;
    let historyLogLimitValue: unknown = null;
    let canLoadMoreHistoryValue: unknown = null;
    let historyBusyValue: unknown = null;
    let historyModeHintValue: unknown = null;
    let historyStateValue: unknown = null;
    let connectionStateValue: unknown = null;
    let errorMessageValue: unknown = 'stale error';

    resetTerminalInstanceSessionState({
      terminalId: 'terminal_1',
      inputForwardEnabledRef,
      inputParseStateRef,
      outputParseStateRef,
      historyLoadedCountRef,
      historyLoadedIdsRef,
      historyBeforeCursorRef,
      replayingHistoryRef,
      pendingOutputChunksRef,
      commandHistoryCacheRef,
      terminalOpenStartedAtRef,
      terminalFirstOutputLoggedRef,
      snapshotVisibleLinesRef,
      snapshotNoMoreLinesRef,
      snapshotLoadingRef,
      supportsSnapshotPagingRef,
      snapshotRequestContextRef,
      setConnectionState: vi.fn((value) => { connectionStateValue = value; }),
      setHistoryState: vi.fn((value) => { historyStateValue = value; }),
      setErrorMessage: vi.fn((value) => { errorMessageValue = value; }),
      setCommandHistory: vi.fn((value) => { commandHistoryValue = value; }),
      setHistoryLogLimit: vi.fn((value) => { historyLogLimitValue = value; }),
      setCanLoadMoreHistory: vi.fn((value) => { canLoadMoreHistoryValue = value; }),
      setHistoryBusy: vi.fn((value) => { historyBusyValue = value; }),
      setHistoryModeHint: vi.fn((value) => { historyModeHintValue = value; }),
    });

    expect(inputParseStateRef.current).toEqual({ lineBuffer: '', skipFollowingLf: false });
    expect(outputParseStateRef.current).toEqual({ lineBuffer: '' });
    expect(historyLoadedCountRef.current).toBe(0);
    expect(Array.from(historyLoadedIdsRef.current)).toEqual([]);
    expect(historyBeforeCursorRef.current).toBeNull();
    expect(replayingHistoryRef.current).toBe(false);
    expect(pendingOutputChunksRef.current).toEqual([]);
    expect(commandHistoryValue).toEqual(commandHistoryCacheRef.current.terminal_1);
    expect(historyLogLimitValue).toBe(0);
    expect(canLoadMoreHistoryValue).toBe(false);
    expect(historyBusyValue).toBe(false);
    expect(historyModeHintValue).toBeNull();
    expect(historyStateValue).toBe('ready');
    expect(connectionStateValue).toBe('disconnected');
    expect(errorMessageValue).toBeNull();
    expect(inputForwardEnabledRef.current).toBe(false);
    expect(typeof terminalOpenStartedAtRef.current).toBe('number');
    expect(terminalFirstOutputLoggedRef.current).toBe(false);
    expect(snapshotVisibleLinesRef.current.terminal_1).toBe(500);
    expect(snapshotNoMoreLinesRef.current.terminal_1).toBe(false);
    expect(snapshotLoadingRef.current).toBe(false);
    expect(supportsSnapshotPagingRef.current).toBe(false);
    expect(snapshotRequestContextRef.current).toBeNull();
  });

  it('cleans up terminal resources and resets lifecycle refs', () => {
    const close = vi.fn();
    const disposeData = vi.fn();
    const disposeScroll = vi.fn();
    const disconnectResizeObserver = vi.fn();
    const disposeTerminal = vi.fn();

    const fitRef = createRef({ fit: vi.fn() } as unknown as import('@xterm/addon-fit').FitAddon);
    const terminalRef = createRef({ dispose: disposeTerminal } as unknown as import('@xterm/xterm').Terminal);
    const socketRef = createRef({
      readyState: WebSocket.OPEN,
      close,
    } as unknown as WebSocket);
    const resizeObserverRef = createRef({
      disconnect: disconnectResizeObserver,
      observe: vi.fn(),
      unobserve: vi.fn(),
    } as unknown as ResizeObserver);
    const dataHandlerRef = createRef({ dispose: disposeData } as unknown as { dispose: () => void });
    const scrollHandlerRef = createRef({ dispose: disposeScroll } as unknown as { dispose: () => void });
    const inputForwardEnabledRef = createRef(true);
    const historyLoadSeqRef = createRef(7);
    const historyLoadedCountRef = createRef(13);
    const historyLoadedIdsRef = createRef(new Set(['log_1']));
    const historyBeforeCursorRef = createRef<string | null>('before_cursor');
    const replayingHistoryRef = createRef(true);
    const pendingOutputChunksRef = createRef(['chunk']);
    const loadHistoryRef = createRef(async () => {});
    const snapshotLoadingRef = createRef(true);
    const supportsSnapshotPagingRef = createRef(true);
    const snapshotRequestContextRef = createRef({
      terminalId: 'terminal_1',
      requestedLines: 1000,
      fromScroll: true,
    });

    let historyStateValue: unknown = null;
    let connectionStateValue: unknown = null;

    cleanupTerminalInstanceSessionState({
      fitRef,
      terminalRef,
      socketRef,
      resizeObserverRef,
      dataHandlerRef,
      scrollHandlerRef,
      inputForwardEnabledRef,
      historyLoadSeqRef,
      historyLoadedCountRef,
      historyLoadedIdsRef,
      historyBeforeCursorRef,
      replayingHistoryRef,
      pendingOutputChunksRef,
      loadHistoryRef,
      snapshotLoadingRef,
      supportsSnapshotPagingRef,
      snapshotRequestContextRef,
      setConnectionState: vi.fn((value) => { connectionStateValue = value; }),
      setHistoryState: vi.fn((value) => { historyStateValue = value; }),
      resizeObserver: {
        disconnect: disconnectResizeObserver,
        observe: vi.fn(),
        unobserve: vi.fn(),
      } as unknown as ResizeObserver,
      term: { dispose: disposeTerminal } as never,
    });

    expect(historyLoadSeqRef.current).toBe(8);
    expect(inputForwardEnabledRef.current).toBe(false);
    expect(loadHistoryRef.current).toBeNull();
    expect(replayingHistoryRef.current).toBe(false);
    expect(pendingOutputChunksRef.current).toEqual([]);
    expect(snapshotLoadingRef.current).toBe(false);
    expect(supportsSnapshotPagingRef.current).toBe(false);
    expect(snapshotRequestContextRef.current).toBeNull();
    expect(historyLoadedCountRef.current).toBe(0);
    expect(Array.from(historyLoadedIdsRef.current)).toEqual([]);
    expect(historyBeforeCursorRef.current).toBeNull();
    expect(close).toHaveBeenCalledTimes(1);
    expect(disposeData).toHaveBeenCalledTimes(1);
    expect(disposeScroll).toHaveBeenCalledTimes(1);
    expect(disconnectResizeObserver).toHaveBeenCalled();
    expect(disposeTerminal).toHaveBeenCalled();
    expect(socketRef.current).toBeNull();
    expect(dataHandlerRef.current).toBeNull();
    expect(scrollHandlerRef.current).toBeNull();
    expect(resizeObserverRef.current).toBeNull();
    expect(terminalRef.current).toBeNull();
    expect(fitRef.current).toBeNull();
    expect(historyStateValue).toBe('idle');
    expect(connectionStateValue).toBe('disconnected');
  });
});
