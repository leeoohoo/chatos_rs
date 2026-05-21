import { describe, expect, it, vi } from 'vitest';

import {
  createTerminalHistoryLoader,
  resolveTerminalHistoryRequest,
} from './terminalHistoryLoader';

const createRef = <T,>(current: T) => ({ current });

describe('terminalHistoryLoader', () => {
  it('computes terminal history request parameters before loading', () => {
    const setCanLoadMoreHistory = vi.fn();
    const setHistoryBusy = vi.fn();
    const request = resolveTerminalHistoryRequest({
      mode: 'more',
      limit: 100,
      historyBeforeCursorRef: createRef<string | null>(null),
      setCanLoadMoreHistory,
      setHistoryBusy,
    });

    expect(request.shouldStop).toBe(true);
    expect(request.requestLimit).toBe(100);
    expect(setCanLoadMoreHistory).toHaveBeenCalledWith(false);
    expect(setHistoryBusy).toHaveBeenCalledWith(false);
  });

  it('short-circuits more loads when there is no cursor', async () => {
    const setCanLoadMoreHistory = vi.fn();
    const setHistoryBusy = vi.fn();
    const setErrorMessage = vi.fn();
    const client = {
      listTerminalLogs: vi.fn(),
    };

    await createTerminalHistoryLoader({
      terminalId: 'terminal_1',
      term: { reset: vi.fn() } as never,
      client: client as never,
      mode: 'more',
      limit: 100,
      isCancelled: () => false,
      isCurrentRequest: () => true,
      inputParseStateRef: createRef({ lineBuffer: '', skipFollowingLf: false }),
      outputParseStateRef: createRef({ lineBuffer: '' }),
      commandSeqRef: createRef(0),
      historyLoadedCountRef: createRef(0),
      historyLoadedIdsRef: createRef(new Set<string>()),
      historyBeforeCursorRef: createRef<string | null>(null),
      replayingHistoryRef: createRef(false),
      pendingOutputChunksRef: createRef([]),
      commandHistoryCacheRef: createRef({}),
      terminalOpenStartedAtRef: createRef(null),
      setHistoryState: vi.fn(),
      setErrorMessage,
      setCommandHistory: vi.fn(),
      setHistoryLogLimit: vi.fn(),
      setCanLoadMoreHistory,
      setHistoryBusy,
      setHistoryModeHint: vi.fn(),
    })();

    expect(client.listTerminalLogs).not.toHaveBeenCalled();
    expect(setCanLoadMoreHistory).toHaveBeenCalledWith(false);
    expect(setHistoryBusy).toHaveBeenCalledWith(false);
    expect(setErrorMessage).toHaveBeenCalledWith(null);
  });
});
