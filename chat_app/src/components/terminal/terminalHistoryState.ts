import { normalizeTerminalLog } from '../../lib/store/helpers/terminals';
import type { TerminalLogResponse } from '../../lib/api/client/types';
import type { TerminalLog } from '../../types';
import {
  mergeCommandHistory,
  normalizeLogTimestamp,
  type CommandHistoryItem,
} from './commandHistory';
import {
  parseCommandHistoryFromLogs,
  TERMINAL_HISTORY_MAX_LIMIT,
  TERMINAL_HISTORY_TAIL_ONLY_HINT,
} from './historyViewUtils';

const OPEN_WEBSOCKET_READY_STATE = 1;

export interface TerminalHistoryPaginationState {
  loadedCount: number;
  loadedIds: Set<string>;
  beforeCursor: string | null;
}

export interface TerminalHistoryLoadResolution {
  normalizedLogs: TerminalLog[];
  uniqueLogs: TerminalLog[];
  nextPagination: TerminalHistoryPaginationState;
  canLoadMoreHistory: boolean;
  reachedHistoryMax: boolean;
  mergedHistory: CommandHistoryItem[];
  outputReplay: string;
  nextSequence: number;
  nextHistoryModeHint: string | null;
  shouldResetHistoryModeHint: boolean;
  shouldReplayOutput: boolean;
  historyLogLimit: number;
  parsedOutputState: ReturnType<typeof parseCommandHistoryFromLogs>['outputState'];
}

export const resolveHistoryRequestLimit = (limit: number): number => (
  Math.max(1, Math.min(limit, TERMINAL_HISTORY_MAX_LIMIT))
);

export const shouldSkipLoadMoreHistory = (
  mode: 'initial' | 'more',
  beforeCursor: string | null,
): boolean => mode === 'more' && !beforeCursor;

export const resolveTerminalSnapshotLoadRequest = ({
  viewportY,
  terminalId,
  supportsSnapshotPaging,
  snapshotLoading,
  noMoreLines,
  currentLines,
  scrollTopLoadThreshold,
  initialLines,
  maxLines,
  pageLines,
  socketReadyState,
}: {
  viewportY: number;
  terminalId: string;
  supportsSnapshotPaging: boolean;
  snapshotLoading: boolean;
  noMoreLines: boolean;
  currentLines: number;
  scrollTopLoadThreshold: number;
  initialLines: number;
  maxLines: number;
  pageLines: number;
  socketReadyState: number | null;
}): {
  shouldRequest: boolean;
  nextLines: number;
  reachedEnd: boolean;
  requestContext: {
    terminalId: string;
    requestedLines: number;
    fromScroll: boolean;
  } | null;
} => {
  if (viewportY > scrollTopLoadThreshold) {
    return {
      shouldRequest: false,
      nextLines: currentLines,
      reachedEnd: false,
      requestContext: null,
    };
  }

  if (!supportsSnapshotPaging || snapshotLoading || noMoreLines || socketReadyState !== OPEN_WEBSOCKET_READY_STATE) {
    return {
      shouldRequest: false,
      nextLines: currentLines,
      reachedEnd: false,
      requestContext: null,
    };
  }

  const effectiveCurrentLines = Math.max(initialLines, currentLines);
  if (effectiveCurrentLines >= maxLines) {
    return {
      shouldRequest: false,
      nextLines: effectiveCurrentLines,
      reachedEnd: true,
      requestContext: null,
    };
  }

  const nextLines = Math.min(maxLines, effectiveCurrentLines + pageLines);
  if (nextLines <= effectiveCurrentLines) {
    return {
      shouldRequest: false,
      nextLines: effectiveCurrentLines,
      reachedEnd: true,
      requestContext: null,
    };
  }

  return {
    shouldRequest: true,
    nextLines,
    reachedEnd: false,
    requestContext: {
      terminalId,
      requestedLines: nextLines,
      fromScroll: true,
    },
  };
};

export const resolveTerminalHistoryLoad = ({
  logs,
  mode,
  requestLimit,
  pagination,
  commandSequence,
  cachedHistory,
}: {
  logs: TerminalLogResponse[];
  mode: 'initial' | 'more';
  requestLimit: number;
  pagination: TerminalHistoryPaginationState;
  commandSequence: number;
  cachedHistory: CommandHistoryItem[];
}): TerminalHistoryLoadResolution => {
  const normalizedLogs = Array.isArray(logs) ? logs.map(normalizeTerminalLog) : [];
  const nextLoadedIds = new Set(pagination.loadedIds);
  const uniqueLogs = normalizedLogs.filter((log) => {
    if (nextLoadedIds.has(log.id)) {
      return false;
    }
    nextLoadedIds.add(log.id);
    return true;
  });

  let nextBeforeCursor = pagination.beforeCursor;
  let nextLoadedCount = pagination.loadedCount;
  if (uniqueLogs.length > 0) {
    nextBeforeCursor = normalizeLogTimestamp(uniqueLogs[0].createdAt);
    nextLoadedCount = Math.min(
      TERMINAL_HISTORY_MAX_LIMIT,
      pagination.loadedCount + uniqueLogs.length,
    );
  }

  const reachedHistoryMax = nextLoadedCount >= TERMINAL_HISTORY_MAX_LIMIT;
  const canLoadMoreHistory = (
    normalizedLogs.length >= requestLimit
    && !reachedHistoryMax
    && Boolean(nextBeforeCursor)
  );

  const parsedHistory = parseCommandHistoryFromLogs(uniqueLogs, commandSequence);
  const outputReplay = parsedHistory.outputLogs.map((log) => log.content || '').join('');
  const mergedHistory = mergeCommandHistory(parsedHistory.commands, cachedHistory);

  return {
    normalizedLogs,
    uniqueLogs,
    nextPagination: {
      loadedCount: nextLoadedCount,
      loadedIds: nextLoadedIds,
      beforeCursor: nextBeforeCursor,
    },
    canLoadMoreHistory,
    reachedHistoryMax,
    mergedHistory,
    outputReplay,
    nextSequence: parsedHistory.nextSequence,
    nextHistoryModeHint: mode === 'more' && uniqueLogs.length > 0
      ? TERMINAL_HISTORY_TAIL_ONLY_HINT
      : null,
    shouldResetHistoryModeHint: mode === 'initial',
    shouldReplayOutput: mode === 'initial' && outputReplay.length > 0,
    historyLogLimit: nextLoadedCount,
    parsedOutputState: parsedHistory.outputState,
  };
};
