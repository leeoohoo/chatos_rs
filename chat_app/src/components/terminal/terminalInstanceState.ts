// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import type { FitAddon } from '@xterm/addon-fit';
import type { Terminal as XTerm } from '@xterm/xterm';

import type {
  CommandHistoryItem,
  CommandHistoryParseState,
  InputCommandParseState,
} from './commandHistory';
import {
  createInitialCommandHistoryParseState,
  createInitialInputCommandParseState,
} from './commandHistory';
import {
  closeWebSocketSafely,
  TERMINAL_SNAPSHOT_INITIAL_LINES,
} from './historyViewUtils';
import type { TerminalConnectionState, TerminalHistoryState } from './TerminalHeader';

export interface TerminalSnapshotRequestContext {
  terminalId: string;
  requestedLines: number;
  fromScroll: boolean;
}

interface TerminalInstanceSessionRefs {
  fitRef: MutableRefObject<FitAddon | null>;
  terminalRef: MutableRefObject<XTerm | null>;
  socketRef: MutableRefObject<WebSocket | null>;
  resizeObserverRef: MutableRefObject<ResizeObserver | null>;
  dataHandlerRef: MutableRefObject<ReturnType<XTerm['onData']> | null>;
  scrollHandlerRef: MutableRefObject<ReturnType<XTerm['onScroll']> | null>;
  inputForwardEnabledRef: MutableRefObject<boolean>;
  inputParseStateRef: MutableRefObject<InputCommandParseState>;
  outputParseStateRef: MutableRefObject<CommandHistoryParseState>;
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
  snapshotRequestContextRef: MutableRefObject<TerminalSnapshotRequestContext | null>;
}

interface TerminalInstanceSessionSetters {
  setConnectionState: Dispatch<SetStateAction<TerminalConnectionState>>;
  setHistoryState: Dispatch<SetStateAction<TerminalHistoryState>>;
  setErrorMessage: Dispatch<SetStateAction<string | null>>;
  setCommandHistory: Dispatch<SetStateAction<CommandHistoryItem[]>>;
  setHistoryLogLimit: Dispatch<SetStateAction<number>>;
  setCanLoadMoreHistory: Dispatch<SetStateAction<boolean>>;
  setHistoryBusy: Dispatch<SetStateAction<boolean>>;
  setHistoryModeHint: Dispatch<SetStateAction<string | null>>;
}

export const resetTerminalInstanceSessionState = ({
  terminalId,
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
}: Pick<
  TerminalInstanceSessionRefs,
  | 'inputForwardEnabledRef'
  | 'inputParseStateRef'
  | 'outputParseStateRef'
  | 'historyLoadedCountRef'
  | 'historyLoadedIdsRef'
  | 'historyBeforeCursorRef'
  | 'replayingHistoryRef'
  | 'pendingOutputChunksRef'
  | 'commandHistoryCacheRef'
  | 'terminalOpenStartedAtRef'
  | 'terminalFirstOutputLoggedRef'
  | 'snapshotVisibleLinesRef'
  | 'snapshotNoMoreLinesRef'
  | 'snapshotLoadingRef'
  | 'supportsSnapshotPagingRef'
  | 'snapshotRequestContextRef'
> & TerminalInstanceSessionSetters & {
  terminalId: string;
}) => {
  inputParseStateRef.current = createInitialInputCommandParseState();
  outputParseStateRef.current = createInitialCommandHistoryParseState();
  setCommandHistory(commandHistoryCacheRef.current[terminalId] ?? []);
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
  snapshotVisibleLinesRef.current[terminalId] = TERMINAL_SNAPSHOT_INITIAL_LINES;
  snapshotNoMoreLinesRef.current[terminalId] = false;
  snapshotLoadingRef.current = false;
  supportsSnapshotPagingRef.current = false;
  snapshotRequestContextRef.current = null;
};

export const cleanupTerminalInstanceSessionState = ({
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
}: Pick<
  TerminalInstanceSessionRefs,
  | 'fitRef'
  | 'terminalRef'
  | 'socketRef'
  | 'resizeObserverRef'
  | 'dataHandlerRef'
  | 'scrollHandlerRef'
  | 'inputForwardEnabledRef'
  | 'historyLoadSeqRef'
  | 'historyLoadedCountRef'
  | 'historyLoadedIdsRef'
  | 'historyBeforeCursorRef'
  | 'replayingHistoryRef'
  | 'pendingOutputChunksRef'
  | 'loadHistoryRef'
  | 'snapshotLoadingRef'
  | 'supportsSnapshotPagingRef'
  | 'snapshotRequestContextRef'
> & Pick<TerminalInstanceSessionSetters, 'setConnectionState' | 'setHistoryState'> & {
  resizeObserver: ResizeObserver;
  term: XTerm;
}) => {
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
