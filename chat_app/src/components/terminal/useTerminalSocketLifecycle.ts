import { useEffect } from 'react';
import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import type { Terminal as XTerm } from '@xterm/xterm';

import type { Terminal } from '../../types';
import { debugLog } from '../../lib/utils';
import type { CommandHistoryParseState } from './commandHistory';
import { createInitialCommandHistoryParseState, parseOutputChunkForCommands } from './commandHistory';
import type { TerminalConnectionState } from './TerminalHeader';
import type { AppendCommandsFn } from './useTerminalAppendCommands';
import { buildWsUrl } from './themeTransport';
import {
  closeWebSocketSafely,
  countSnapshotLines,
  TERMINAL_SNAPSHOT_INITIAL_LINES,
  TERMINAL_SNAPSHOT_MAX_LINES,
} from './historyViewUtils';

interface UseTerminalSocketLifecycleParams {
  currentTerminal: Terminal | null;
  apiBaseUrl: string;
  accessToken?: string | null;
  connectSeq: number;
  loadTerminals: () => void | Promise<any>;
  appendCommands: AppendCommandsFn;
  terminalRef: MutableRefObject<XTerm | null>;
  socketRef: MutableRefObject<WebSocket | null>;
  inputForwardEnabledRef: MutableRefObject<boolean>;
  outputParseStateRef: MutableRefObject<CommandHistoryParseState>;
  replayingHistoryRef: MutableRefObject<boolean>;
  pendingOutputChunksRef: MutableRefObject<string[]>;
  terminalFirstOutputLoggedRef: MutableRefObject<boolean>;
  terminalOpenStartedAtRef: MutableRefObject<number | null>;
  appliedSnapshotRef: MutableRefObject<string>;
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
  setErrorMessage: Dispatch<SetStateAction<string | null>>;
}

export const useTerminalSocketLifecycle = ({
  currentTerminal,
  apiBaseUrl,
  accessToken,
  connectSeq,
  loadTerminals,
  appendCommands,
  terminalRef,
  socketRef,
  inputForwardEnabledRef,
  outputParseStateRef,
  replayingHistoryRef,
  pendingOutputChunksRef,
  terminalFirstOutputLoggedRef,
  terminalOpenStartedAtRef,
  appliedSnapshotRef,
  snapshotVisibleLinesRef,
  snapshotNoMoreLinesRef,
  snapshotLoadingRef,
  supportsSnapshotPagingRef,
  snapshotRequestContextRef,
  setConnectionState,
  setErrorMessage,
}: UseTerminalSocketLifecycleParams) => {
  useEffect(() => {
    if (!currentTerminal) return;

    const wsUrl = buildWsUrl(apiBaseUrl, `/terminals/${currentTerminal.id}/ws`, accessToken);
    setConnectionState('connecting');
    inputForwardEnabledRef.current = false;
    appliedSnapshotRef.current = '';
    snapshotLoadingRef.current = false;
    supportsSnapshotPagingRef.current = false;
    snapshotRequestContextRef.current = null;

    const ws = new WebSocket(wsUrl);
    socketRef.current = ws;

    ws.onopen = () => {
      if (socketRef.current !== ws) {
        return;
      }
      setConnectionState('connected');
      const term = terminalRef.current;
      if (term) {
        ws.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
      }
      inputForwardEnabledRef.current = true;
    };

    ws.onmessage = (event) => {
      if (socketRef.current !== ws) {
        return;
      }

      try {
        const payload = JSON.parse(event.data);
        if (payload?.type === 'snapshot' && typeof payload.data === 'string') {
          const terminalId = currentTerminal.id;
          const requestContext = snapshotRequestContextRef.current;
          const currentSnapshot = appliedSnapshotRef.current;
          const nextSnapshot = payload.data;
          const previousLineCount = countSnapshotLines(currentSnapshot);
          const nextLineCount = countSnapshotLines(nextSnapshot);
          const hasMoreLines = nextLineCount > previousLineCount;

          if (nextSnapshot !== currentSnapshot) {
            appliedSnapshotRef.current = nextSnapshot;
            const term = terminalRef.current;
            if (term) {
              term.reset();
              term.write(nextSnapshot);
              const parsedSnapshot = parseOutputChunkForCommands(
                nextSnapshot,
                createInitialCommandHistoryParseState(),
              );
              outputParseStateRef.current = parsedSnapshot.nextState;
              appendCommands(parsedSnapshot.commands, new Date().toISOString(), 'correct');
              if (requestContext?.terminalId === terminalId && requestContext.fromScroll) {
                term.scrollToTop();
              }
            }
          }

          if (requestContext?.terminalId === terminalId) {
            snapshotVisibleLinesRef.current[terminalId] = requestContext.requestedLines;
            if (!hasMoreLines || requestContext.requestedLines >= TERMINAL_SNAPSHOT_MAX_LINES) {
              snapshotNoMoreLinesRef.current[terminalId] = true;
            } else {
              snapshotNoMoreLinesRef.current[terminalId] = false;
            }
          } else {
            snapshotVisibleLinesRef.current[terminalId] = Math.max(
              TERMINAL_SNAPSHOT_INITIAL_LINES,
              Math.min(TERMINAL_SNAPSHOT_MAX_LINES, nextLineCount),
            );
            snapshotNoMoreLinesRef.current[terminalId] = false;
          }
          snapshotLoadingRef.current = false;
          snapshotRequestContextRef.current = null;
        } else if (payload?.type === 'output') {
          const outputData = payload.data ?? '';
          if (replayingHistoryRef.current) {
            pendingOutputChunksRef.current.push(outputData);
            return;
          }
          if (
            !terminalFirstOutputLoggedRef.current
            && terminalOpenStartedAtRef.current
            && typeof outputData === 'string'
            && outputData.length > 0
          ) {
            terminalFirstOutputLoggedRef.current = true;
            debugLog('[Perf] terminal first realtime output', {
              terminalId: currentTerminal.id,
              elapsedMs: Date.now() - terminalOpenStartedAtRef.current,
            });
          }
          terminalRef.current?.write(outputData);

          const parsed = parseOutputChunkForCommands(outputData, outputParseStateRef.current);
          outputParseStateRef.current = parsed.nextState;
          appendCommands(parsed.commands, new Date().toISOString(), 'correct');
        } else if (payload?.type === 'exit') {
          inputForwardEnabledRef.current = false;
          setConnectionState('disconnected');
          loadTerminals();
        } else if (payload?.type === 'state') {
          if (typeof payload.snapshot_paging === 'boolean') {
            supportsSnapshotPagingRef.current = payload.snapshot_paging;
          }
          loadTerminals();
        } else if (payload?.type === 'error') {
          setErrorMessage(payload.error || '终端发生错误');
          inputForwardEnabledRef.current = false;
          setConnectionState('error');
        }
      } catch (err) {
        void err;
      }
    };

    ws.onerror = () => {
      if (socketRef.current !== ws) {
        return;
      }
      inputForwardEnabledRef.current = false;
      snapshotLoadingRef.current = false;
      supportsSnapshotPagingRef.current = false;
      snapshotRequestContextRef.current = null;
      setConnectionState('error');
    };

    ws.onclose = () => {
      if (socketRef.current !== ws) {
        return;
      }
      inputForwardEnabledRef.current = false;
      snapshotLoadingRef.current = false;
      supportsSnapshotPagingRef.current = false;
      snapshotRequestContextRef.current = null;
      setConnectionState('disconnected');
      loadTerminals();
    };

    return () => {
      inputForwardEnabledRef.current = false;
      snapshotLoadingRef.current = false;
      supportsSnapshotPagingRef.current = false;
      snapshotRequestContextRef.current = null;
      if (socketRef.current === ws) {
        socketRef.current = null;
      }
      closeWebSocketSafely(ws);
    };
  }, [
    accessToken,
    apiBaseUrl,
    appendCommands,
    appliedSnapshotRef,
    connectSeq,
    currentTerminal?.id,
    inputForwardEnabledRef,
    loadTerminals,
    outputParseStateRef,
    pendingOutputChunksRef,
    replayingHistoryRef,
    setConnectionState,
    setErrorMessage,
    snapshotLoadingRef,
    snapshotNoMoreLinesRef,
    snapshotRequestContextRef,
    snapshotVisibleLinesRef,
    socketRef,
    supportsSnapshotPagingRef,
    terminalFirstOutputLoggedRef,
    terminalOpenStartedAtRef,
    terminalRef,
  ]);
};
