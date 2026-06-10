import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import type { Terminal as XTerm } from '@xterm/xterm';

import type { TerminalLogResponse } from '../../lib/api/client/types';
import type {
  CommandHistoryItem,
  CommandHistoryParseState,
  InputCommandParseState,
} from './commandHistory';
import { writeToTerminalInChunks } from './commandHistory';
import {
  resolveHistoryRequestLimit,
  resolveTerminalHistoryLoad,
  shouldSkipLoadMoreHistory,
} from './terminalHistoryState';
import { resetTerminalInputParseState } from './terminalLifecycleHandlers';
import type { TerminalHistoryState } from './TerminalHeader';
import { debugLog } from '../../lib/utils';
import type { TerminalRuntimeTextRef } from './terminalRuntimeText';

export interface TerminalHistoryClient {
  listTerminalLogs(
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string },
  ): Promise<TerminalLogResponse[]>;
}

export const createTerminalHistoryLoader = ({
  terminalId,
  term,
  client,
  mode,
  limit,
  isCancelled,
  isCurrentRequest,
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
  runtimeTextRef,
}: {
  terminalId: string;
  term: XTerm;
  client: TerminalHistoryClient;
  mode: 'initial' | 'more';
  limit: number;
  isCancelled: () => boolean;
  isCurrentRequest: () => boolean;
  inputParseStateRef: MutableRefObject<InputCommandParseState>;
  outputParseStateRef: MutableRefObject<CommandHistoryParseState>;
  commandSeqRef: MutableRefObject<number>;
  historyLoadedCountRef: MutableRefObject<number>;
  historyLoadedIdsRef: MutableRefObject<Set<string>>;
  historyBeforeCursorRef: MutableRefObject<string | null>;
  replayingHistoryRef: MutableRefObject<boolean>;
  pendingOutputChunksRef: MutableRefObject<string[]>;
  commandHistoryCacheRef: MutableRefObject<Record<string, CommandHistoryItem[]>>;
  terminalOpenStartedAtRef: MutableRefObject<number | null>;
  setHistoryState: Dispatch<SetStateAction<TerminalHistoryState>>;
  setErrorMessage: Dispatch<SetStateAction<string | null>>;
  setCommandHistory: Dispatch<SetStateAction<CommandHistoryItem[]>>;
  setHistoryLogLimit: Dispatch<SetStateAction<number>>;
  setCanLoadMoreHistory: Dispatch<SetStateAction<boolean>>;
  setHistoryBusy: Dispatch<SetStateAction<boolean>>;
  setHistoryModeHint: Dispatch<SetStateAction<string | null>>;
  runtimeTextRef?: TerminalRuntimeTextRef;
}) => async () => {
  beginTerminalHistoryLoad({
    mode,
    setHistoryBusy,
    setErrorMessage,
  });

  try {
    const request = resolveTerminalHistoryRequest({
      mode,
      limit,
      historyBeforeCursorRef,
      setCanLoadMoreHistory,
      setHistoryBusy,
    });
    if (request.shouldStop) {
      return;
    }

    const logs = await client.listTerminalLogs(terminalId, {
      limit: request.requestLimit,
      ...(request.requestBefore ? { before: request.requestBefore } : {}),
    });
    if (isCancelled() || !isCurrentRequest()) {
      return;
    }

    await applyTerminalHistoryLoadSuccess({
      terminalId,
      term,
      mode,
      logs,
      requestLimit: request.requestLimit,
      inputParseStateRef,
      outputParseStateRef,
      commandSeqRef,
      historyLoadedCountRef,
      historyLoadedIdsRef,
      historyBeforeCursorRef,
      replayingHistoryRef,
      commandHistoryCacheRef,
      terminalOpenStartedAtRef,
      setHistoryState,
      setCommandHistory,
      setHistoryLogLimit,
      setCanLoadMoreHistory,
      setHistoryModeHint,
    });
  } catch (error) {
    handleTerminalHistoryLoadError({
      error,
      mode,
      isCancelled,
      isCurrentRequest,
      setHistoryState,
      setCanLoadMoreHistory,
      setErrorMessage,
      runtimeTextRef,
    });
  } finally {
    finalizeTerminalHistoryLoad({
      isCancelled,
      isCurrentRequest,
      replayingHistoryRef,
      pendingOutputChunksRef,
      setHistoryBusy,
    });
  }
};

export const resolveTerminalHistoryRequest = ({
  mode,
  limit,
  historyBeforeCursorRef,
  setCanLoadMoreHistory,
  setHistoryBusy,
}: {
  mode: 'initial' | 'more';
  limit: number;
  historyBeforeCursorRef: MutableRefObject<string | null>;
  setCanLoadMoreHistory: Dispatch<SetStateAction<boolean>>;
  setHistoryBusy: Dispatch<SetStateAction<boolean>>;
}): {
  shouldStop: boolean;
  requestLimit: number;
  requestBefore: string | null;
} => {
  const requestLimit = resolveHistoryRequestLimit(limit);
  const requestBefore = mode === 'more' ? historyBeforeCursorRef.current : null;
  if (shouldSkipLoadMoreHistory(mode, requestBefore)) {
    setCanLoadMoreHistory(false);
    setHistoryBusy(false);
    return {
      shouldStop: true,
      requestLimit,
      requestBefore,
    };
  }
  return {
    shouldStop: false,
    requestLimit,
    requestBefore,
  };
};

export const createTerminalHistoryLoadExecutor = ({
  terminalId,
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
  runtimeTextRef,
}: {
  terminalId: string;
  term: XTerm;
  client: TerminalHistoryClient;
  cancelledRef: MutableRefObject<boolean>;
  terminalRef: MutableRefObject<XTerm | null>;
  historyLoadSeqRef: MutableRefObject<number>;
  inputParseStateRef: MutableRefObject<InputCommandParseState>;
  outputParseStateRef: MutableRefObject<CommandHistoryParseState>;
  commandSeqRef: MutableRefObject<number>;
  historyLoadedCountRef: MutableRefObject<number>;
  historyLoadedIdsRef: MutableRefObject<Set<string>>;
  historyBeforeCursorRef: MutableRefObject<string | null>;
  replayingHistoryRef: MutableRefObject<boolean>;
  pendingOutputChunksRef: MutableRefObject<string[]>;
  commandHistoryCacheRef: MutableRefObject<Record<string, CommandHistoryItem[]>>;
  terminalOpenStartedAtRef: MutableRefObject<number | null>;
  setHistoryState: Dispatch<SetStateAction<TerminalHistoryState>>;
  setErrorMessage: Dispatch<SetStateAction<string | null>>;
  setCommandHistory: Dispatch<SetStateAction<CommandHistoryItem[]>>;
  setHistoryLogLimit: Dispatch<SetStateAction<number>>;
  setCanLoadMoreHistory: Dispatch<SetStateAction<boolean>>;
  setHistoryBusy: Dispatch<SetStateAction<boolean>>;
  setHistoryModeHint: Dispatch<SetStateAction<string | null>>;
  runtimeTextRef?: TerminalRuntimeTextRef;
}) => async (
  limit: number,
  mode: 'initial' | 'more',
): Promise<void> => {
  const requestSeq = historyLoadSeqRef.current + 1;
  historyLoadSeqRef.current = requestSeq;
  const isCurrentRequest = () => requestSeq === historyLoadSeqRef.current;
  const isCancelled = () => cancelledRef.current || terminalRef.current !== term;
  await createTerminalHistoryLoader({
    terminalId,
    term,
    client,
    mode,
    limit,
    isCancelled,
    isCurrentRequest,
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
    runtimeTextRef,
  })();
};

const beginTerminalHistoryLoad = ({
  mode,
  setHistoryBusy,
  setErrorMessage,
}: {
  mode: 'initial' | 'more';
  setHistoryBusy: Dispatch<SetStateAction<boolean>>;
  setErrorMessage: Dispatch<SetStateAction<string | null>>;
}) => {
  if (mode === 'more') {
    setHistoryBusy(true);
  }
  setErrorMessage(null);
};

const applyTerminalHistoryLoadSuccess = async ({
  terminalId,
  term,
  mode,
  logs,
  requestLimit,
  inputParseStateRef,
  outputParseStateRef,
  commandSeqRef,
  historyLoadedCountRef,
  historyLoadedIdsRef,
  historyBeforeCursorRef,
  replayingHistoryRef,
  commandHistoryCacheRef,
  terminalOpenStartedAtRef,
  setHistoryState,
  setCommandHistory,
  setHistoryLogLimit,
  setCanLoadMoreHistory,
  setHistoryModeHint,
}: {
  terminalId: string;
  term: XTerm;
  mode: 'initial' | 'more';
  logs: TerminalLogResponse[];
  requestLimit: number;
  inputParseStateRef: MutableRefObject<InputCommandParseState>;
  outputParseStateRef: MutableRefObject<CommandHistoryParseState>;
  commandSeqRef: MutableRefObject<number>;
  historyLoadedCountRef: MutableRefObject<number>;
  historyLoadedIdsRef: MutableRefObject<Set<string>>;
  historyBeforeCursorRef: MutableRefObject<string | null>;
  replayingHistoryRef: MutableRefObject<boolean>;
  commandHistoryCacheRef: MutableRefObject<Record<string, CommandHistoryItem[]>>;
  terminalOpenStartedAtRef: MutableRefObject<number | null>;
  setHistoryState: Dispatch<SetStateAction<TerminalHistoryState>>;
  setCommandHistory: Dispatch<SetStateAction<CommandHistoryItem[]>>;
  setHistoryLogLimit: Dispatch<SetStateAction<number>>;
  setCanLoadMoreHistory: Dispatch<SetStateAction<boolean>>;
  setHistoryModeHint: Dispatch<SetStateAction<string | null>>;
}) => {
  const resolved = resolveTerminalHistoryLoad({
    logs,
    mode,
    requestLimit,
    pagination: {
      loadedCount: historyLoadedCountRef.current,
      loadedIds: historyLoadedIdsRef.current,
      beforeCursor: historyBeforeCursorRef.current,
    },
    commandSequence: commandSeqRef.current,
    cachedHistory: commandHistoryCacheRef.current[terminalId] ?? [],
  });
  historyLoadedIdsRef.current = resolved.nextPagination.loadedIds;
  historyBeforeCursorRef.current = resolved.nextPagination.beforeCursor;
  historyLoadedCountRef.current = resolved.nextPagination.loadedCount;
  commandSeqRef.current = resolved.nextSequence;

  if (mode === 'initial' && resolved.shouldReplayOutput) {
    term.reset();
    await writeToTerminalInChunks(term, resolved.outputReplay);
  }

  resetTerminalInputParseState(inputParseStateRef);
  setCanLoadMoreHistory(resolved.canLoadMoreHistory);
  setCommandHistory(resolved.mergedHistory);
  commandHistoryCacheRef.current[terminalId] = resolved.mergedHistory;

  if (resolved.shouldResetHistoryModeHint) {
    outputParseStateRef.current = resolved.parsedOutputState;
    setHistoryModeHint(null);
  } else if (resolved.nextHistoryModeHint) {
    setHistoryModeHint(resolved.nextHistoryModeHint);
  }

  replayingHistoryRef.current = false;
  setHistoryLogLimit(resolved.historyLogLimit);
  setHistoryState('ready');
  if (mode === 'initial' && terminalOpenStartedAtRef.current) {
    debugLog('[Perf] terminal history ready', {
      terminalId,
      elapsedMs: Date.now() - terminalOpenStartedAtRef.current,
      loadedLogs: resolved.historyLogLimit,
    });
  }
};

const handleTerminalHistoryLoadError = ({
  error,
  mode,
  isCancelled,
  isCurrentRequest,
  setHistoryState,
  setCanLoadMoreHistory,
  setErrorMessage,
  runtimeTextRef,
}: {
  error: unknown;
  mode: 'initial' | 'more';
  isCancelled: () => boolean;
  isCurrentRequest: () => boolean;
  setHistoryState: Dispatch<SetStateAction<TerminalHistoryState>>;
  setCanLoadMoreHistory: Dispatch<SetStateAction<boolean>>;
  setErrorMessage: Dispatch<SetStateAction<string | null>>;
  runtimeTextRef?: TerminalRuntimeTextRef;
}) => {
  if (isCancelled() || !isCurrentRequest()) {
    return;
  }
  console.error('Failed to load terminal history:', error);
  if (mode === 'initial') {
    setHistoryState('error');
    setCanLoadMoreHistory(false);
  }
  setErrorMessage(error instanceof Error ? error.message : (runtimeTextRef?.current.historyLoadFailed || 'Failed to load history'));
};

const finalizeTerminalHistoryLoad = ({
  isCancelled,
  isCurrentRequest,
  replayingHistoryRef,
  pendingOutputChunksRef,
  setHistoryBusy,
}: {
  isCancelled: () => boolean;
  isCurrentRequest: () => boolean;
  replayingHistoryRef: MutableRefObject<boolean>;
  pendingOutputChunksRef: MutableRefObject<string[]>;
  setHistoryBusy: Dispatch<SetStateAction<boolean>>;
}) => {
  if (isCancelled() || !isCurrentRequest()) {
    return;
  }
  replayingHistoryRef.current = false;
  pendingOutputChunksRef.current = [];
  setHistoryBusy(false);
};
