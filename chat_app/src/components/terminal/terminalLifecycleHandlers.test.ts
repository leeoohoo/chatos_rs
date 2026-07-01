// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import {
  createTerminalDataHandler,
  createTerminalResizeObserverHandler,
  createTerminalScrollHandler,
  resetTerminalInputParseState,
} from './terminalLifecycleHandlers';

const createRef = <T,>(current: T) => ({ current });

describe('terminalLifecycleHandlers', () => {
  it('appends parsed commands and forwards socket payloads for terminal input', () => {
    const appendCommands = vi.fn();
    const send = vi.fn();
    const handler = createTerminalDataHandler({
      term: {
        buffer: {
          active: {
            cursorY: 0,
            getLine: () => ({
              translateToString: () => '/workspace $ git   status',
              isWrapped: false,
            }),
          },
        },
      } as never,
      socketRef: createRef({
        readyState: WebSocket.OPEN,
        send,
      } as unknown as WebSocket),
      inputForwardEnabledRef: createRef(true),
      inputParseStateRef: createRef({
        lineBuffer: 'git   status',
        skipFollowingLf: false,
      }),
      appendCommands,
    });

    handler('\r');

    expect(appendCommands).toHaveBeenNthCalledWith(
      1,
      ['git   status'],
      expect.any(String),
      'append',
    );
    expect(appendCommands).toHaveBeenNthCalledWith(
      2,
      ['git status'],
      expect.any(String),
      'correct',
    );
    expect(send).toHaveBeenNthCalledWith(1, JSON.stringify({ type: 'command', command: 'git status' }));
    expect(send).toHaveBeenNthCalledWith(2, JSON.stringify({ type: 'input', data: '\r' }));
  });

  it('requests more snapshot lines only when scroll reaches the top and paging is still available', () => {
    const send = vi.fn();
    const snapshotVisibleLinesRef = createRef<Record<string, number>>({ terminal_1: 500 });
    const snapshotNoMoreLinesRef = createRef<Record<string, boolean>>({ terminal_1: false });
    const snapshotLoadingRef = createRef(false);
    const snapshotRequestContextRef = createRef<{
      terminalId: string;
      requestedLines: number;
      fromScroll: boolean;
    } | null>(null);

    const handler = createTerminalScrollHandler({
      terminalId: 'terminal_1',
      socketRef: createRef({
        readyState: WebSocket.OPEN,
        send,
      } as unknown as WebSocket),
      snapshotVisibleLinesRef,
      snapshotNoMoreLinesRef,
      snapshotLoadingRef,
      supportsSnapshotPagingRef: createRef(true),
      snapshotRequestContextRef,
      scrollTopLoadThreshold: 0,
      initialLines: 500,
      maxLines: 1000,
      pageLines: 500,
    });

    handler(0);

    expect(snapshotLoadingRef.current).toBe(true);
    expect(snapshotRequestContextRef.current).toEqual({
      terminalId: 'terminal_1',
      requestedLines: 1000,
      fromScroll: true,
    });
    expect(send).toHaveBeenCalledWith(JSON.stringify({ type: 'snapshot', lines: 1000 }));
  });

  it('marks snapshot exhaustion instead of sending another request once the max line budget is reached', () => {
    const send = vi.fn();
    const snapshotNoMoreLinesRef = createRef<Record<string, boolean>>({ terminal_1: false });
    const handler = createTerminalScrollHandler({
      terminalId: 'terminal_1',
      socketRef: createRef({
        readyState: WebSocket.OPEN,
        send,
      } as unknown as WebSocket),
      snapshotVisibleLinesRef: createRef<Record<string, number>>({ terminal_1: 1000 }),
      snapshotNoMoreLinesRef,
      snapshotLoadingRef: createRef(false),
      supportsSnapshotPagingRef: createRef(true),
      snapshotRequestContextRef: createRef(null),
      scrollTopLoadThreshold: 0,
      initialLines: 500,
      maxLines: 1000,
      pageLines: 500,
    });

    handler(0);

    expect(snapshotNoMoreLinesRef.current.terminal_1).toBe(true);
    expect(send).not.toHaveBeenCalled();
  });

  it('fits the terminal locally and sends resize payloads only when the socket is open', () => {
    const fit = vi.fn();
    const send = vi.fn();
    const handler = createTerminalResizeObserverHandler({
      fitRef: createRef({ fit } as unknown as import('@xterm/addon-fit').FitAddon),
      terminalRef: createRef({
        cols: 132,
        rows: 40,
      } as unknown as import('@xterm/xterm').Terminal),
      socketRef: createRef({
        readyState: WebSocket.OPEN,
        send,
      } as unknown as WebSocket),
    });

    handler();

    expect(fit).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith(JSON.stringify({ type: 'resize', cols: 132, rows: 40 }));
  });

  it('resets the incremental input parser state', () => {
    const inputParseStateRef = createRef({
      lineBuffer: 'stale',
      skipFollowingLf: true,
    });

    resetTerminalInputParseState(inputParseStateRef);

    expect(inputParseStateRef.current).toEqual({
      lineBuffer: '',
      skipFollowingLf: false,
    });
  });
});
