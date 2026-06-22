import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import type { Terminal as XTerm } from '@xterm/xterm';

import { debugLog } from '../../lib/utils';
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
import type { TerminalLogResponse } from '../../lib/api/client/types';

export interface TerminalHistoryClient {
  listTerminalLogs(
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string },
  ): Promise<TerminalLogResponse[]>;
}

interface TerminalHistoryLoadContext {
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
}

export const beginTerminalHistoryLoad = ({
  mode,
  setHistoryBusy,
  setErrorMessage,
}: Pick<TerminalHistoryLoadContext, 'mode' | 'setHistoryBusy' | 'setErrorMessage'>): void => {
  if (mode === 'more') {
    setHistoryBusy(true);
  }
  setErrorMessage(null);
};

export const resolveTerminalHistoryRequest = ({
  mode,
  limit,
  historyBeforeCursorRef,
  setCanLoadMoreHistory,
  setHistoryBusy,
}: Pick<
  TerminalHistoryLoadContext,
  | 'mode'
  | 'limit'
  | 'historyBeforeCursorRef'
  | 'setCanLoadMoreHistory'
  | 'setHistoryBusy'
>): {
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

export const applyTerminalHistoryLoadSuccess = async ({
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
}: Omit<TerminalHistoryLoadContext, 'client' | 'limit' | 'isCancelled' | 'isCurrentRequest' | 'pendingOutputChunksRef' | 'setErrorMessage' | 'setHistoryBusy'> & {
  logs: TerminalLogResponse[];
  requestLimit: number;
}): Promise<void> => {
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

export const handleTerminalHistoryLoadError = ({
  error,
  mode,
  isCancelled,
  isCurrentRequest,
  setHistoryState,
  setCanLoadMoreHistory,
  setErrorMessage,
  fallbackErrorMessage = 'Failed to load history',
}: Pick<
  TerminalHistoryLoadContext,
  | 'mode'
  | 'isCancelled'
  | 'isCurrentRequest'
  | 'setHistoryState'
  | 'setCanLoadMoreHistory'
  | 'setErrorMessage'
> & {
  error: unknown;
  fallbackErrorMessage?: string;
}): boolean => {
  if (isCancelled() || !isCurrentRequest()) {
    return false;
  }
  console.error('Failed to load terminal history:', error);
  if (mode === 'initial') {
    setHistoryState('error');
    setCanLoadMoreHistory(false);
  }
  setErrorMessage(error instanceof Error ? error.message : fallbackErrorMessage);
  return true;
};

export const finalizeTerminalHistoryLoad = ({
  isCancelled,
  isCurrentRequest,
  replayingHistoryRef,
  pendingOutputChunksRef,
  setHistoryBusy,
}: Pick<
  TerminalHistoryLoadContext,
  | 'isCancelled'
  | 'isCurrentRequest'
  | 'replayingHistoryRef'
  | 'pendingOutputChunksRef'
  | 'setHistoryBusy'
>): boolean => {
  if (isCancelled() || !isCurrentRequest()) {
    return false;
  }
  replayingHistoryRef.current = false;
  pendingOutputChunksRef.current = [];
  setHistoryBusy(false);
  return true;
};
