import { useCallback, useEffect } from 'react';
import type { MutableRefObject } from 'react';

import type { TerminalLogResponse } from '../../lib/api/client/types';
import type { Terminal } from '../../types';
import type { CommandHistoryItem } from './commandHistory';
import type { TerminalConnectionState, TerminalHistoryState } from './TerminalHeader';
import {
  TERMINAL_HISTORY_MAX_LIMIT,
  TERMINAL_HISTORY_PAGE_SIZE,
} from './historyViewUtils';
import { useTerminalAppendCommands } from './useTerminalAppendCommands';
import { useTerminalInstanceLifecycle } from './useTerminalInstanceLifecycle';
import { useTerminalSocketLifecycle } from './useTerminalSocketLifecycle';
import { useTerminalViewState } from './useTerminalViewState';

interface TerminalApiClient {
  getBaseUrl(): string;
  listTerminalLogs(
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string },
  ): Promise<TerminalLogResponse[]>;
}

interface UseTerminalRuntimeParams {
  currentTerminal: Terminal | null;
  loadTerminals: () => void | Promise<any>;
  client: TerminalApiClient;
  accessToken?: string | null;
  actualTheme: 'light' | 'dark';
}

interface UseTerminalRuntimeResult {
  containerRef: MutableRefObject<HTMLDivElement | null>;
  connectionState: TerminalConnectionState;
  historyState: TerminalHistoryState;
  historyBusy: boolean;
  canLoadMoreHistory: boolean;
  historyModeHint: string | null;
  errorMessage: string | null;
  commandHistoryCount: number;
  displayHistory: CommandHistoryItem[];
  reconnect: () => void;
  loadMoreHistory: () => void;
}

export const useTerminalRuntime = ({
  currentTerminal,
  loadTerminals,
  client,
  accessToken,
  actualTheme,
}: UseTerminalRuntimeParams): UseTerminalRuntimeResult => {
  const state = useTerminalViewState(actualTheme);
  const apiBaseUrl = client.getBaseUrl();

  const appendCommands = useTerminalAppendCommands({
    currentTerminalId: currentTerminal?.id ?? null,
    setCommandHistory: state.setCommandHistory,
    commandHistoryCacheRef: state.commandHistoryCacheRef,
    commandSeqRef: state.commandSeqRef,
  });

  useTerminalInstanceLifecycle({
    currentTerminal,
    client,
    themeColors: state.themeColors,
    themeColorsRef: state.themeColorsRef,
    terminalRef: state.terminalRef,
    fitRef: state.fitRef,
    containerRef: state.containerRef,
    socketRef: state.socketRef,
    resizeObserverRef: state.resizeObserverRef,
    dataHandlerRef: state.dataHandlerRef,
    scrollHandlerRef: state.scrollHandlerRef,
    inputForwardEnabledRef: state.inputForwardEnabledRef,
    inputParseStateRef: state.inputParseStateRef,
    outputParseStateRef: state.outputParseStateRef,
    commandSeqRef: state.commandSeqRef,
    historyLoadSeqRef: state.historyLoadSeqRef,
    historyLoadedCountRef: state.historyLoadedCountRef,
    historyLoadedIdsRef: state.historyLoadedIdsRef,
    historyBeforeCursorRef: state.historyBeforeCursorRef,
    replayingHistoryRef: state.replayingHistoryRef,
    pendingOutputChunksRef: state.pendingOutputChunksRef,
    loadHistoryRef: state.loadHistoryRef,
    commandHistoryCacheRef: state.commandHistoryCacheRef,
    terminalOpenStartedAtRef: state.terminalOpenStartedAtRef,
    terminalFirstOutputLoggedRef: state.terminalFirstOutputLoggedRef,
    snapshotVisibleLinesRef: state.snapshotVisibleLinesRef,
    snapshotNoMoreLinesRef: state.snapshotNoMoreLinesRef,
    snapshotLoadingRef: state.snapshotLoadingRef,
    supportsSnapshotPagingRef: state.supportsSnapshotPagingRef,
    snapshotRequestContextRef: state.snapshotRequestContextRef,
    setConnectionState: state.setConnectionState,
    setHistoryState: state.setHistoryState,
    setErrorMessage: state.setErrorMessage,
    setCommandHistory: state.setCommandHistory,
    setHistoryLogLimit: state.setHistoryLogLimit,
    setCanLoadMoreHistory: state.setCanLoadMoreHistory,
    setHistoryBusy: state.setHistoryBusy,
    setHistoryModeHint: state.setHistoryModeHint,
    appendCommands,
  });

  useTerminalSocketLifecycle({
    currentTerminal,
    apiBaseUrl,
    accessToken,
    connectSeq: state.connectSeq,
    loadTerminals,
    appendCommands,
    terminalRef: state.terminalRef,
    socketRef: state.socketRef,
    inputForwardEnabledRef: state.inputForwardEnabledRef,
    outputParseStateRef: state.outputParseStateRef,
    replayingHistoryRef: state.replayingHistoryRef,
    pendingOutputChunksRef: state.pendingOutputChunksRef,
    terminalFirstOutputLoggedRef: state.terminalFirstOutputLoggedRef,
    terminalOpenStartedAtRef: state.terminalOpenStartedAtRef,
    appliedSnapshotRef: state.appliedSnapshotRef,
    snapshotVisibleLinesRef: state.snapshotVisibleLinesRef,
    snapshotNoMoreLinesRef: state.snapshotNoMoreLinesRef,
    snapshotLoadingRef: state.snapshotLoadingRef,
    supportsSnapshotPagingRef: state.supportsSnapshotPagingRef,
    snapshotRequestContextRef: state.snapshotRequestContextRef,
    setConnectionState: state.setConnectionState,
    setErrorMessage: state.setErrorMessage,
  });

  useEffect(() => {
    loadTerminals();
  }, [loadTerminals]);

  const reconnect = useCallback(() => {
    state.setConnectSeq((prev) => prev + 1);
  }, [state.setConnectSeq]);

  const loadMoreHistory = useCallback(() => {
    if (!currentTerminal?.id) {
      return;
    }
    const remaining = TERMINAL_HISTORY_MAX_LIMIT - state.historyLogLimit;
    if (remaining <= 0) {
      return;
    }
    const pageSize = Math.min(TERMINAL_HISTORY_PAGE_SIZE, remaining);
    void state.loadHistoryRef.current?.(pageSize, 'more');
  }, [currentTerminal?.id, state.historyLogLimit, state.loadHistoryRef]);

  return {
    containerRef: state.containerRef,
    connectionState: state.connectionState,
    historyState: state.historyState,
    historyBusy: state.historyBusy,
    canLoadMoreHistory: state.canLoadMoreHistory,
    historyModeHint: state.historyModeHint,
    errorMessage: state.errorMessage,
    commandHistoryCount: state.commandHistory.length,
    displayHistory: state.displayHistory,
    reconnect,
    loadMoreHistory,
  };
};
