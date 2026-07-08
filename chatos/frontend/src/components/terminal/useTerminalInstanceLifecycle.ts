// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect } from 'react';
import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import type { ITheme } from '@xterm/xterm';

import type { Terminal } from '../../types';
import type {
  CommandHistoryItem,
  CommandHistoryParseState,
  InputCommandParseState,
} from './commandHistory';
import type { TerminalConnectionState, TerminalHistoryState } from './TerminalHeader';
import type { AppendCommandsFn } from './useTerminalAppendCommands';
import {
  TERMINAL_HISTORY_INITIAL_LIMIT,
  TERMINAL_SCROLL_TOP_LOAD_THRESHOLD,
  TERMINAL_SNAPSHOT_INITIAL_LINES,
  TERMINAL_SNAPSHOT_MAX_LINES,
  TERMINAL_SNAPSHOT_PAGE_LINES,
} from './historyViewUtils';
import {
  cleanupTerminalInstanceSessionState,
  resetTerminalInstanceSessionState,
} from './terminalInstanceState';
import {
  createTerminalDataHandler,
  createTerminalResizeObserverHandler,
  createTerminalScrollHandler,
} from './terminalLifecycleHandlers';
import {
  createTerminalHistoryLoadExecutor,
  type TerminalHistoryClient,
} from './terminalHistoryLoader';
import type { TerminalRuntimeTextRef } from './terminalRuntimeText';

interface UseTerminalInstanceLifecycleParams {
  currentTerminal: Terminal | null;
  client: TerminalHistoryClient;
  themeColors: ITheme;
  themeColorsRef: MutableRefObject<ITheme>;
  terminalRef: MutableRefObject<XTerm | null>;
  fitRef: MutableRefObject<FitAddon | null>;
  containerRef: MutableRefObject<HTMLDivElement | null>;
  socketRef: MutableRefObject<WebSocket | null>;
  resizeObserverRef: MutableRefObject<ResizeObserver | null>;
  dataHandlerRef: MutableRefObject<ReturnType<XTerm['onData']> | null>;
  scrollHandlerRef: MutableRefObject<ReturnType<XTerm['onScroll']> | null>;
  inputForwardEnabledRef: MutableRefObject<boolean>;
  inputParseStateRef: MutableRefObject<InputCommandParseState>;
  outputParseStateRef: MutableRefObject<CommandHistoryParseState>;
  commandSeqRef: MutableRefObject<number>;
  historyLoadSeqRef: MutableRefObject<number>;
  historyLoadedCountRef: MutableRefObject<number>;
  historyLoadedIdsRef: MutableRefObject<Set<string>>;
  historyBeforeCursorRef: MutableRefObject<string | null>;
  replayingHistoryRef: MutableRefObject<boolean>;
  pendingOutputChunksRef: MutableRefObject<string[]>;
  loadHistoryRef: MutableRefObject<((limit: number, mode: 'initial' | 'more') => Promise<void>) | null>;
  commandHistoryCacheRef: MutableRefObject<Record<string, CommandHistoryItem[]>>;
  terminalOpenStartedAtRef: MutableRefObject<number | null>;
  terminalFirstOutputLoggedRef: MutableRefObject<boolean>;
  snapshotVisibleLinesRef: MutableRefObject<Record<string, number>>;
  snapshotNoMoreLinesRef: MutableRefObject<Record<string, boolean>>;
  snapshotLoadingRef: MutableRefObject<boolean>;
  supportsSnapshotPagingRef: MutableRefObject<boolean>;
  snapshotRequestContextRef: MutableRefObject<{
    terminalId: string;
    requestedLines: number;
    fromScroll: boolean;
  } | null>;
  setConnectionState: Dispatch<SetStateAction<TerminalConnectionState>>;
  setHistoryState: Dispatch<SetStateAction<TerminalHistoryState>>;
  setErrorMessage: Dispatch<SetStateAction<string | null>>;
  setCommandHistory: Dispatch<SetStateAction<CommandHistoryItem[]>>;
  setHistoryLogLimit: Dispatch<SetStateAction<number>>;
  setCanLoadMoreHistory: Dispatch<SetStateAction<boolean>>;
  setHistoryBusy: Dispatch<SetStateAction<boolean>>;
  setHistoryModeHint: Dispatch<SetStateAction<string | null>>;
  appendCommands: AppendCommandsFn;
  runtimeTextRef: TerminalRuntimeTextRef;
}

export const useTerminalInstanceLifecycle = ({
  currentTerminal,
  client,
  themeColors,
  themeColorsRef,
  terminalRef,
  fitRef,
  containerRef,
  socketRef,
  resizeObserverRef,
  dataHandlerRef,
  scrollHandlerRef,
  inputForwardEnabledRef,
  inputParseStateRef,
  outputParseStateRef,
  commandSeqRef,
  historyLoadSeqRef,
  historyLoadedCountRef,
  historyLoadedIdsRef,
  historyBeforeCursorRef,
  replayingHistoryRef,
  pendingOutputChunksRef,
  loadHistoryRef,
  commandHistoryCacheRef,
  terminalOpenStartedAtRef,
  terminalFirstOutputLoggedRef,
  snapshotVisibleLinesRef,
  snapshotNoMoreLinesRef,
  snapshotLoadingRef,
  supportsSnapshotPagingRef,
  snapshotRequestContextRef,
  setConnectionState,
  setHistoryState,
  setErrorMessage,
  setCommandHistory,
  setHistoryLogLimit,
  setCanLoadMoreHistory,
  setHistoryBusy,
  setHistoryModeHint,
  appendCommands,
  runtimeTextRef,
}: UseTerminalInstanceLifecycleParams) => {
  useEffect(() => {
    themeColorsRef.current = themeColors;
    const term = terminalRef.current;
    if (term) {
      term.options.theme = themeColors;
    }
  }, [terminalRef, themeColors, themeColorsRef]);

  useEffect(() => {
    if (!currentTerminal || !containerRef.current) {
      loadHistoryRef.current = null;
      return;
    }

    const cancelledRef = { current: false };

    resetTerminalInstanceSessionState({
      terminalId: currentTerminal.id,
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
      setConnectionState,
      setHistoryState,
      setErrorMessage,
      setCommandHistory,
      setHistoryLogLimit,
      setCanLoadMoreHistory,
      setHistoryBusy,
      setHistoryModeHint,
    });

    const term = new XTerm({
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
      fontSize: 13,
      lineHeight: 1.2,
      cursorBlink: true,
      scrollback: 10000,
      theme: themeColorsRef.current,
    });
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);

    term.open(containerRef.current);
    fitAddon.fit();
    term.focus();

    terminalRef.current = term;
    fitRef.current = fitAddon;

    dataHandlerRef.current = term.onData(createTerminalDataHandler({
      term,
      socketRef,
      inputForwardEnabledRef,
      inputParseStateRef,
      appendCommands,
    }));

    scrollHandlerRef.current = term.onScroll(createTerminalScrollHandler({
      terminalId: currentTerminal.id,
      socketRef,
      snapshotVisibleLinesRef,
      snapshotNoMoreLinesRef,
      snapshotLoadingRef,
      supportsSnapshotPagingRef,
      snapshotRequestContextRef,
      scrollTopLoadThreshold: TERMINAL_SCROLL_TOP_LOAD_THRESHOLD,
      initialLines: TERMINAL_SNAPSHOT_INITIAL_LINES,
      maxLines: TERMINAL_SNAPSHOT_MAX_LINES,
      pageLines: TERMINAL_SNAPSHOT_PAGE_LINES,
    }));

    const resizeObserver = new ResizeObserver(createTerminalResizeObserverHandler({
      fitRef,
      terminalRef,
      socketRef,
    }));
    resizeObserver.observe(containerRef.current);
    resizeObserverRef.current = resizeObserver;

    const loadHistory = createTerminalHistoryLoadExecutor({
      terminalId: currentTerminal.id,
      term,
      client,
      cancelledRef,
      terminalRef,
      historyLoadSeqRef,
      inputParseStateRef,
      outputParseStateRef,
      commandSeqRef,
      historyLoadedCountRef,
      historyLoadedIdsRef,
      historyBeforeCursorRef,
      replayingHistoryRef,
      pendingOutputChunksRef,
      commandHistoryCacheRef,
      terminalOpenStartedAtRef,
      setHistoryState,
      setErrorMessage,
      setCommandHistory,
      setHistoryLogLimit,
      setCanLoadMoreHistory,
      setHistoryBusy,
      setHistoryModeHint,
    });

    loadHistoryRef.current = loadHistory;
    void loadHistory(TERMINAL_HISTORY_INITIAL_LIMIT, 'initial');

    return () => {
      cancelledRef.current = true;
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
        setConnectionState,
        setHistoryState,
        resizeObserver,
        term,
      });
    };
  }, [
    appendCommands,
    client,
    commandHistoryCacheRef,
    commandSeqRef,
    containerRef,
    currentTerminal?.id,
    dataHandlerRef,
    fitRef,
    historyBeforeCursorRef,
    historyLoadedCountRef,
    historyLoadedIdsRef,
    historyLoadSeqRef,
    inputForwardEnabledRef,
    inputParseStateRef,
    loadHistoryRef,
    outputParseStateRef,
    pendingOutputChunksRef,
    replayingHistoryRef,
    resizeObserverRef,
    runtimeTextRef,
    scrollHandlerRef,
    setCanLoadMoreHistory,
    setCommandHistory,
    setConnectionState,
    setErrorMessage,
    setHistoryBusy,
    setHistoryLogLimit,
    setHistoryModeHint,
    setHistoryState,
    snapshotLoadingRef,
    snapshotNoMoreLinesRef,
    snapshotRequestContextRef,
    snapshotVisibleLinesRef,
    socketRef,
    supportsSnapshotPagingRef,
    terminalFirstOutputLoggedRef,
    terminalOpenStartedAtRef,
    terminalRef,
    themeColorsRef,
  ]);
};
