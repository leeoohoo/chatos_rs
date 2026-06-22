import { describe, expect, it, vi } from 'vitest';

import {
  applyTerminalErrorMessage,
  applyTerminalExitMessage,
  applyTerminalOutputMessage,
  applyTerminalSnapshotMessage,
  applyTerminalSocketOpen,
  applyTerminalStateMessage,
  resetTerminalSocketConnectionState,
  resetTerminalSocketSnapshotState,
} from './terminalSocketState';

const createRef = <T,>(current: T) => ({ current });

describe('terminalSocketState', () => {
  it('resets snapshot-related refs before a new websocket session starts', () => {
    const inputForwardEnabledRef = createRef(true);
    const appliedSnapshotRef = createRef('stale');
    const snapshotLoadingRef = createRef(true);
    const supportsSnapshotPagingRef = createRef(true);
    const snapshotRequestContextRef = createRef({
      terminalId: 'terminal_1',
      requestedLines: 1000,
      fromScroll: true,
    });

    resetTerminalSocketSnapshotState({
      inputForwardEnabledRef,
      appliedSnapshotRef,
      snapshotLoadingRef,
      supportsSnapshotPagingRef,
      snapshotRequestContextRef,
    });

    expect(inputForwardEnabledRef.current).toBe(false);
    expect(appliedSnapshotRef.current).toBe('');
    expect(snapshotLoadingRef.current).toBe(false);
    expect(supportsSnapshotPagingRef.current).toBe(false);
    expect(snapshotRequestContextRef.current).toBeNull();
  });

  it('marks the socket connected and sends the initial resize payload on open', () => {
    const send = vi.fn();
    const setConnectionState = vi.fn();
    const inputForwardEnabledRef = createRef(false);

    applyTerminalSocketOpen({
      term: {
        cols: 120,
        rows: 32,
      } as unknown as import('@xterm/xterm').Terminal,
      ws: { send } as unknown as WebSocket,
      inputForwardEnabledRef,
      setConnectionState,
    });

    expect(setConnectionState).toHaveBeenCalledWith('connected');
    expect(send).toHaveBeenCalledWith(JSON.stringify({ type: 'resize', cols: 120, rows: 32 }));
    expect(inputForwardEnabledRef.current).toBe(true);
  });

  it('replays snapshot output, updates pagination refs, and clears loading state', () => {
    const reset = vi.fn();
    const write = vi.fn();
    const scrollToTop = vi.fn();
    const appendCommands = vi.fn();
    const outputParseStateRef = createRef({ lineBuffer: '' });
    const appliedSnapshotRef = createRef('');
    const snapshotVisibleLinesRef = createRef<Record<string, number>>({});
    const snapshotNoMoreLinesRef = createRef<Record<string, boolean>>({});
    const snapshotLoadingRef = createRef(true);
    const snapshotRequestContextRef = createRef({
      terminalId: 'terminal_1',
      requestedLines: 1000,
      fromScroll: true,
    });

    applyTerminalSnapshotMessage({
      terminalId: 'terminal_1',
      snapshot: '/workspace $ git status\nOn branch main\n',
      requestContext: snapshotRequestContextRef.current,
      term: {
        reset,
        write,
        scrollToTop,
      } as unknown as import('@xterm/xterm').Terminal,
      outputParseStateRef,
      appendCommands,
      appliedSnapshotRef,
      snapshotVisibleLinesRef,
      snapshotNoMoreLinesRef,
      snapshotLoadingRef,
      snapshotRequestContextRef,
    });

    expect(reset).toHaveBeenCalledTimes(1);
    expect(write).toHaveBeenCalledWith('/workspace $ git status\nOn branch main\n');
    expect(scrollToTop).toHaveBeenCalledTimes(1);
    expect(appendCommands).toHaveBeenCalledWith(['git status'], expect.any(String), 'correct');
    expect(appliedSnapshotRef.current).toBe('/workspace $ git status\nOn branch main\n');
    expect(snapshotVisibleLinesRef.current.terminal_1).toBe(1000);
    expect(snapshotNoMoreLinesRef.current.terminal_1).toBe(false);
    expect(snapshotLoadingRef.current).toBe(false);
    expect(snapshotRequestContextRef.current).toBeNull();
  });

  it('buffers output while history replay is active and otherwise parses commands from realtime output', () => {
    const appendCommands = vi.fn();
    const write = vi.fn();
    const replayingHistoryRef = createRef(true);
    const pendingOutputChunksRef = createRef<string[]>([]);
    const terminalFirstOutputLoggedRef = createRef(false);
    const terminalOpenStartedAtRef = createRef<number | null>(Date.now() - 10);
    const outputParseStateRef = createRef({ lineBuffer: '' });

    applyTerminalOutputMessage({
      terminalId: 'terminal_1',
      outputData: 'buffered chunk',
      term: {
        write,
      } as unknown as import('@xterm/xterm').Terminal,
      outputParseStateRef,
      replayingHistoryRef,
      pendingOutputChunksRef,
      terminalFirstOutputLoggedRef,
      terminalOpenStartedAtRef,
      appendCommands,
    });

    expect(pendingOutputChunksRef.current).toEqual(['buffered chunk']);
    expect(write).not.toHaveBeenCalled();

    replayingHistoryRef.current = false;
    applyTerminalOutputMessage({
      terminalId: 'terminal_1',
      outputData: '/workspace $ pwd\n',
      term: {
        write,
      } as unknown as import('@xterm/xterm').Terminal,
      outputParseStateRef,
      replayingHistoryRef,
      pendingOutputChunksRef,
      terminalFirstOutputLoggedRef,
      terminalOpenStartedAtRef,
      appendCommands,
    });

    expect(write).toHaveBeenCalledWith('/workspace $ pwd\n');
    expect(appendCommands).toHaveBeenCalledWith(['pwd'], expect.any(String), 'correct');
    expect(terminalFirstOutputLoggedRef.current).toBe(true);
  });

  it('applies terminal state, exit, error, and connection cleanup consistently', () => {
    const inputForwardEnabledRef = createRef(true);
    const snapshotLoadingRef = createRef(true);
    const supportsSnapshotPagingRef = createRef(true);
    const snapshotRequestContextRef = createRef({
      terminalId: 'terminal_1',
      requestedLines: 1000,
      fromScroll: true,
    });
    const setConnectionState = vi.fn();
    const setErrorMessage = vi.fn();

    applyTerminalStateMessage({
      snapshotPaging: true,
      supportsSnapshotPagingRef,
    });
    expect(supportsSnapshotPagingRef.current).toBe(true);

    applyTerminalExitMessage({
      inputForwardEnabledRef,
      setConnectionState,
    });
    expect(inputForwardEnabledRef.current).toBe(false);
    expect(setConnectionState).toHaveBeenCalledWith('disconnected');

    inputForwardEnabledRef.current = true;
    applyTerminalErrorMessage({
      message: '终端发生错误',
      inputForwardEnabledRef,
      setConnectionState,
      setErrorMessage,
    });
    expect(setErrorMessage).toHaveBeenCalledWith('终端发生错误');
    expect(inputForwardEnabledRef.current).toBe(false);
    expect(setConnectionState).toHaveBeenCalledWith('error');

    inputForwardEnabledRef.current = true;
    resetTerminalSocketConnectionState({
      inputForwardEnabledRef,
      snapshotLoadingRef,
      supportsSnapshotPagingRef,
      snapshotRequestContextRef,
    });
    expect(inputForwardEnabledRef.current).toBe(false);
    expect(snapshotLoadingRef.current).toBe(false);
    expect(supportsSnapshotPagingRef.current).toBe(false);
    expect(snapshotRequestContextRef.current).toBeNull();
  });
});
