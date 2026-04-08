import { useEffect } from 'react';
import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';

import type { TerminalLogResponse } from '../../lib/api/client/types';
import type { Terminal } from '../../types';
import { normalizeTerminalLog } from '../../lib/store/helpers/terminals';
import { debugLog } from '../../lib/utils';
import type {
  CommandHistoryItem,
  CommandHistoryParseState,
  InputCommandParseState,
} from './commandHistory';
import {
  canCommandBeUsed,
  createInitialCommandHistoryParseState,
  createInitialInputCommandParseState,
  extractCommandFromTerminalBuffer,
  mergeCommandHistory,
  normalizeCommandForCompare,
  normalizeLogTimestamp,
  parseInputChunkForCommands,
} from './commandHistory';
import type { TerminalConnectionState, TerminalHistoryState } from './TerminalHeader';
import type { AppendCommandsFn } from './useTerminalAppendCommands';
import {
  closeWebSocketSafely,
  parseCommandHistoryFromLogs,
  TERMINAL_HISTORY_INITIAL_LIMIT,
  TERMINAL_HISTORY_MAX_LIMIT,
  TERMINAL_HISTORY_TAIL_ONLY_HINT,
  TERMINAL_SCROLL_TOP_LOAD_THRESHOLD,
  TERMINAL_SNAPSHOT_INITIAL_LINES,
  TERMINAL_SNAPSHOT_MAX_LINES,
  TERMINAL_SNAPSHOT_PAGE_LINES,
} from './historyViewUtils';

interface TerminalApiClient {
  listTerminalLogs(
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string },
  ): Promise<TerminalLogResponse[]>;
}

interface UseTerminalInstanceLifecycleParams {
  currentTerminal: Terminal | null;
  client: TerminalApiClient;
  themeColors: any;
  themeColorsRef: MutableRefObject<any>;
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

    let cancelled = false;

    inputParseStateRef.current = createInitialInputCommandParseState();
    outputParseStateRef.current = createInitialCommandHistoryParseState();
    const cachedHistory = commandHistoryCacheRef.current[currentTerminal.id] ?? [];
    setCommandHistory(cachedHistory);
    pendingOutputChunksRef.current = [];
    historyLoadedCountRef.current = 0;
    historyLoadedIdsRef.current = new Set();
    historyBeforeCursorRef.current = null;
    replayingHistoryRef.current = false;
    setHistoryLogLimit(0);
    setCanLoadMoreHistory(false);
    setHistoryBusy(false);
    setHistoryModeHint(null);
    setHistoryState('ready');
    setConnectionState('disconnected');
    setErrorMessage(null);
    inputForwardEnabledRef.current = false;
    terminalOpenStartedAtRef.current = Date.now();
    terminalFirstOutputLoggedRef.current = false;
    snapshotVisibleLinesRef.current[currentTerminal.id] = TERMINAL_SNAPSHOT_INITIAL_LINES;
    snapshotNoMoreLinesRef.current[currentTerminal.id] = false;
    snapshotLoadingRef.current = false;
    supportsSnapshotPagingRef.current = false;
    snapshotRequestContextRef.current = null;

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

    dataHandlerRef.current = term.onData((data) => {
      if (!inputForwardEnabledRef.current) {
        return;
      }

      const submittedCommand = (data.includes('\r') || data.includes('\n'))
        ? extractCommandFromTerminalBuffer(term)
        : null;

      const parsedInput = parseInputChunkForCommands(data, inputParseStateRef.current);
      inputParseStateRef.current = parsedInput.nextState;
      appendCommands(parsedInput.commands, new Date().toISOString(), 'append');

      const normalizedSubmittedCommand = submittedCommand
        ? normalizeCommandForCompare(submittedCommand)
        : '';
      if (canCommandBeUsed(normalizedSubmittedCommand)) {
        appendCommands([normalizedSubmittedCommand], new Date().toISOString(), 'correct');
      }

      const ws = socketRef.current;
      if (ws && ws.readyState === WebSocket.OPEN) {
        if (canCommandBeUsed(normalizedSubmittedCommand)) {
          ws.send(JSON.stringify({ type: 'command', command: normalizedSubmittedCommand }));
        }
        ws.send(JSON.stringify({ type: 'input', data }));
      }
    });

    scrollHandlerRef.current = term.onScroll((viewportY) => {
      if (viewportY > TERMINAL_SCROLL_TOP_LOAD_THRESHOLD) {
        return;
      }

      const terminalId = currentTerminal.id;
      if (
        !supportsSnapshotPagingRef.current
        || snapshotLoadingRef.current
        || snapshotNoMoreLinesRef.current[terminalId]
      ) {
        return;
      }

      const currentLines = snapshotVisibleLinesRef.current[terminalId] ?? TERMINAL_SNAPSHOT_INITIAL_LINES;
      if (currentLines >= TERMINAL_SNAPSHOT_MAX_LINES) {
        snapshotNoMoreLinesRef.current[terminalId] = true;
        return;
      }

      const ws = socketRef.current;
      if (!ws || ws.readyState !== WebSocket.OPEN) {
        return;
      }

      const nextLines = Math.min(TERMINAL_SNAPSHOT_MAX_LINES, currentLines + TERMINAL_SNAPSHOT_PAGE_LINES);
      if (nextLines <= currentLines) {
        snapshotNoMoreLinesRef.current[terminalId] = true;
        return;
      }

      snapshotLoadingRef.current = true;
      snapshotRequestContextRef.current = {
        terminalId,
        requestedLines: nextLines,
        fromScroll: true,
      };
      ws.send(JSON.stringify({ type: 'snapshot', lines: nextLines }));
    });

    const resizeObserver = new ResizeObserver(() => {
      const fit = fitRef.current;
      if (!fit) return;
      fit.fit();
      const active = socketRef.current;
      if (active && active.readyState === WebSocket.OPEN && terminalRef.current) {
        active.send(JSON.stringify({ type: 'resize', cols: terminalRef.current.cols, rows: terminalRef.current.rows }));
      }
    });
    resizeObserver.observe(containerRef.current);
    resizeObserverRef.current = resizeObserver;

    const loadHistory = async (
      limit: number,
      mode: 'initial' | 'more',
    ) => {
      const requestSeq = historyLoadSeqRef.current + 1;
      historyLoadSeqRef.current = requestSeq;
      const isCurrentRequest = () => requestSeq === historyLoadSeqRef.current;

      if (mode === 'more') {
        setHistoryBusy(true);
      }
      setErrorMessage(null);

      try {
        const requestLimit = Math.max(1, Math.min(limit, TERMINAL_HISTORY_MAX_LIMIT));
        const requestBefore = mode === 'more' ? historyBeforeCursorRef.current : null;
        if (mode === 'more' && !requestBefore) {
          setCanLoadMoreHistory(false);
          setHistoryBusy(false);
          return;
        }
        const logs = await client.listTerminalLogs(currentTerminal.id, {
          limit: requestLimit,
          ...(requestBefore ? { before: requestBefore } : {}),
        });
        if (cancelled || !isCurrentRequest() || terminalRef.current !== term) {
          return;
        }

        const normalized = Array.isArray(logs) ? logs.map(normalizeTerminalLog) : [];
        const uniqueLogs = normalized.filter((log) => {
          if (historyLoadedIdsRef.current.has(log.id)) {
            return false;
          }
          historyLoadedIdsRef.current.add(log.id);
          return true;
        });
        if (uniqueLogs.length > 0) {
          historyBeforeCursorRef.current = normalizeLogTimestamp(uniqueLogs[0].createdAt);
          historyLoadedCountRef.current = Math.min(
            TERMINAL_HISTORY_MAX_LIMIT,
            historyLoadedCountRef.current + uniqueLogs.length,
          );
        }
        const reachedHistoryMax = historyLoadedCountRef.current >= TERMINAL_HISTORY_MAX_LIMIT;
        setCanLoadMoreHistory(
          normalized.length >= requestLimit
          && !reachedHistoryMax
          && Boolean(historyBeforeCursorRef.current),
        );
        const parsedHistory = parseCommandHistoryFromLogs(uniqueLogs, commandSeqRef.current);
        commandSeqRef.current = parsedHistory.nextSequence;

        inputParseStateRef.current = createInitialInputCommandParseState();
        const cachedMergedHistory = commandHistoryCacheRef.current[currentTerminal.id] ?? [];
        const mergedHistory = mergeCommandHistory(parsedHistory.commands, cachedMergedHistory);
        setCommandHistory(mergedHistory);
        commandHistoryCacheRef.current[currentTerminal.id] = mergedHistory;

        if (mode === 'initial') {
          outputParseStateRef.current = parsedHistory.outputState;
          setHistoryModeHint(null);
        } else if (uniqueLogs.length > 0) {
          setHistoryModeHint(TERMINAL_HISTORY_TAIL_ONLY_HINT);
        }

        replayingHistoryRef.current = false;
        setHistoryLogLimit(historyLoadedCountRef.current);
        setHistoryState('ready');
        if (mode === 'initial' && terminalOpenStartedAtRef.current) {
          debugLog('[Perf] terminal history ready', {
            terminalId: currentTerminal.id,
            elapsedMs: Date.now() - terminalOpenStartedAtRef.current,
            loadedLogs: historyLoadedCountRef.current,
          });
        }
      } catch (error) {
        if (cancelled || !isCurrentRequest()) {
          return;
        }
        console.error('Failed to load terminal history:', error);
        if (mode === 'initial') {
          setHistoryState('error');
          setCanLoadMoreHistory(false);
        }
        setErrorMessage(error instanceof Error ? error.message : '加载历史失败');
      } finally {
        if (cancelled || !isCurrentRequest()) {
          return;
        }
        replayingHistoryRef.current = false;
        pendingOutputChunksRef.current = [];
        setHistoryBusy(false);
      }
    };

    loadHistoryRef.current = loadHistory;
    void loadHistory(TERMINAL_HISTORY_INITIAL_LIMIT, 'initial');

    return () => {
      cancelled = true;
      historyLoadSeqRef.current += 1;
      inputForwardEnabledRef.current = false;
      loadHistoryRef.current = null;
      replayingHistoryRef.current = false;
      pendingOutputChunksRef.current = [];
      snapshotLoadingRef.current = false;
      supportsSnapshotPagingRef.current = false;
      snapshotRequestContextRef.current = null;
      historyLoadedCountRef.current = 0;
      historyLoadedIdsRef.current = new Set();
      historyBeforeCursorRef.current = null;
      closeWebSocketSafely(socketRef.current);
      socketRef.current = null;
      dataHandlerRef.current?.dispose();
      dataHandlerRef.current = null;
      scrollHandlerRef.current?.dispose();
      scrollHandlerRef.current = null;
      resizeObserver.disconnect();
      resizeObserverRef.current = null;
      term.dispose();
      terminalRef.current = null;
      fitRef.current = null;
      setHistoryState('idle');
      setConnectionState('disconnected');
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
