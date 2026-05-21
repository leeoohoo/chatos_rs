import { useEffect } from 'react';
import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import type { Terminal as XTerm } from '@xterm/xterm';

import { getRealtimeConnectionStateSnapshot } from '../../lib/realtime/state';
import type { Terminal } from '../../types';
import type { CommandHistoryParseState } from './commandHistory';
import type { TerminalConnectionState } from './TerminalHeader';
import type { AppendCommandsFn } from './useTerminalAppendCommands';
import { buildWsUrl } from './themeTransport';
import { closeWebSocketSafely } from './historyViewUtils';
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

interface UseTerminalSocketLifecycleParams {
  currentTerminal: Terminal | null;
  apiBaseUrl: string;
  accessToken?: string | null;
  connectSeq: number;
  loadTerminals: () => void | Promise<unknown>;
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

const shouldFallbackRefreshTerminals = (): boolean => (
  getRealtimeConnectionStateSnapshot() !== 'connected'
);

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
    resetTerminalSocketSnapshotState({
      inputForwardEnabledRef,
      appliedSnapshotRef,
      snapshotLoadingRef,
      supportsSnapshotPagingRef,
      snapshotRequestContextRef,
    });

    const ws = new WebSocket(wsUrl);
    socketRef.current = ws;

    ws.onopen = () => {
      if (socketRef.current !== ws) {
        return;
      }
      applyTerminalSocketOpen({
        term: terminalRef.current,
        ws,
        inputForwardEnabledRef,
        setConnectionState,
      });
    };

    ws.onmessage = (event) => {
      if (socketRef.current !== ws) {
        return;
      }

      try {
        const payload = JSON.parse(event.data);
        if (payload?.type === 'snapshot' && typeof payload.data === 'string') {
          applyTerminalSnapshotMessage({
            terminalId: currentTerminal.id,
            snapshot: payload.data,
            requestContext: snapshotRequestContextRef.current,
            term: terminalRef.current,
            outputParseStateRef,
            appendCommands,
            appliedSnapshotRef,
            snapshotVisibleLinesRef,
            snapshotNoMoreLinesRef,
            snapshotLoadingRef,
            snapshotRequestContextRef,
          });
        } else if (payload?.type === 'output') {
          applyTerminalOutputMessage({
            terminalId: currentTerminal.id,
            outputData: String(payload.data ?? ''),
            term: terminalRef.current,
            outputParseStateRef,
            replayingHistoryRef,
            pendingOutputChunksRef,
            terminalFirstOutputLoggedRef,
            terminalOpenStartedAtRef,
            appendCommands,
          });
        } else if (payload?.type === 'exit') {
          applyTerminalExitMessage({
            inputForwardEnabledRef,
            setConnectionState,
          });
          if (shouldFallbackRefreshTerminals()) {
            loadTerminals();
          }
        } else if (payload?.type === 'state') {
          applyTerminalStateMessage({
            snapshotPaging: payload.snapshot_paging,
            supportsSnapshotPagingRef,
          });
          if (shouldFallbackRefreshTerminals()) {
            loadTerminals();
          }
        } else if (payload?.type === 'error') {
          applyTerminalErrorMessage({
            message: payload.error || '终端发生错误',
            inputForwardEnabledRef,
            setConnectionState,
            setErrorMessage,
          });
        }
      } catch (err) {
        void err;
      }
    };

    ws.onerror = () => {
      if (socketRef.current !== ws) {
        return;
      }
      resetTerminalSocketConnectionState({
        inputForwardEnabledRef,
        snapshotLoadingRef,
        supportsSnapshotPagingRef,
        snapshotRequestContextRef,
      });
      setErrorMessage('终端实时连接失败，请点击“重连”；如果仍无输出，可先查看右侧命令历史并刷新运行状态。');
      setConnectionState('error');
    };

    ws.onclose = () => {
      if (socketRef.current !== ws) {
        return;
      }
      resetTerminalSocketConnectionState({
        inputForwardEnabledRef,
        snapshotLoadingRef,
        supportsSnapshotPagingRef,
        snapshotRequestContextRef,
      });
      setConnectionState('disconnected');
      if (shouldFallbackRefreshTerminals()) {
        loadTerminals();
      }
    };

    return () => {
      resetTerminalSocketConnectionState({
        inputForwardEnabledRef,
        snapshotLoadingRef,
        supportsSnapshotPagingRef,
        snapshotRequestContextRef,
      });
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
