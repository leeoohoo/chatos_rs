import { useMemo, useRef, useState } from 'react';
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
import type { TerminalConnectionState, TerminalHistoryState } from './TerminalHeader';
import { getThemeColors } from './themeTransport';

export const useTerminalViewState = (actualTheme: 'light' | 'dark') => {
  const terminalRef = useRef<XTerm | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  const dataHandlerRef = useRef<ReturnType<XTerm['onData']> | null>(null);
  const scrollHandlerRef = useRef<ReturnType<XTerm['onScroll']> | null>(null);
  const inputForwardEnabledRef = useRef(false);
  const inputParseStateRef = useRef<InputCommandParseState>(createInitialInputCommandParseState());
  const outputParseStateRef = useRef<CommandHistoryParseState>(createInitialCommandHistoryParseState());
  const commandSeqRef = useRef(0);
  const historyLoadSeqRef = useRef(0);
  const historyLoadedCountRef = useRef(0);
  const historyLoadedIdsRef = useRef<Set<string>>(new Set());
  const historyBeforeCursorRef = useRef<string | null>(null);
  const replayingHistoryRef = useRef(false);
  const pendingOutputChunksRef = useRef<string[]>([]);
  const loadHistoryRef = useRef<((limit: number, mode: 'initial' | 'more') => Promise<void>) | null>(null);
  const themeColorsRef = useRef(getThemeColors(actualTheme));
  const commandHistoryCacheRef = useRef<Record<string, CommandHistoryItem[]>>({});
  const terminalOpenStartedAtRef = useRef<number | null>(null);
  const terminalFirstOutputLoggedRef = useRef(false);
  const appliedSnapshotRef = useRef<string>('');
  const snapshotVisibleLinesRef = useRef<Record<string, number>>({});
  const snapshotNoMoreLinesRef = useRef<Record<string, boolean>>({});
  const snapshotLoadingRef = useRef(false);
  const supportsSnapshotPagingRef = useRef(false);
  const snapshotRequestContextRef = useRef<{
    terminalId: string;
    requestedLines: number;
    fromScroll: boolean;
  } | null>(null);

  const [connectionState, setConnectionState] = useState<TerminalConnectionState>('disconnected');
  const [historyState, setHistoryState] = useState<TerminalHistoryState>('idle');
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [connectSeq, setConnectSeq] = useState(0);
  const [commandHistory, setCommandHistory] = useState<CommandHistoryItem[]>([]);
  const [historyLogLimit, setHistoryLogLimit] = useState(0);
  const [canLoadMoreHistory, setCanLoadMoreHistory] = useState(false);
  const [historyBusy, setHistoryBusy] = useState(false);
  const [historyModeHint, setHistoryModeHint] = useState<string | null>(null);

  const themeColors = useMemo(() => getThemeColors(actualTheme), [actualTheme]);
  const displayHistory = useMemo(() => [...commandHistory].reverse(), [commandHistory]);

  return {
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
    themeColorsRef,
    commandHistoryCacheRef,
    terminalOpenStartedAtRef,
    terminalFirstOutputLoggedRef,
    appliedSnapshotRef,
    snapshotVisibleLinesRef,
    snapshotNoMoreLinesRef,
    snapshotLoadingRef,
    supportsSnapshotPagingRef,
    snapshotRequestContextRef,
    connectionState,
    setConnectionState,
    historyState,
    setHistoryState,
    errorMessage,
    setErrorMessage,
    connectSeq,
    setConnectSeq,
    commandHistory,
    setCommandHistory,
    historyLogLimit,
    setHistoryLogLimit,
    canLoadMoreHistory,
    setCanLoadMoreHistory,
    historyBusy,
    setHistoryBusy,
    historyModeHint,
    setHistoryModeHint,
    themeColors,
    displayHistory,
  };
};
