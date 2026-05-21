import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import type { Terminal as XTerm } from '@xterm/xterm';

import { debugLog } from '../../lib/utils';
import type { CommandHistoryParseState } from './commandHistory';
import { createInitialCommandHistoryParseState, parseOutputChunkForCommands } from './commandHistory';
import {
  countSnapshotLines,
  TERMINAL_SNAPSHOT_INITIAL_LINES,
  TERMINAL_SNAPSHOT_MAX_LINES,
} from './historyViewUtils';
import type { TerminalConnectionState } from './TerminalHeader';
import type { AppendCommandsFn } from './useTerminalAppendCommands';

export interface TerminalSocketSnapshotRequestContext {
  terminalId: string;
  requestedLines: number;
  fromScroll: boolean;
}

export interface TerminalSocketSnapshotStateRefs {
  inputForwardEnabledRef: MutableRefObject<boolean>;
  appliedSnapshotRef: MutableRefObject<string>;
  snapshotVisibleLinesRef: MutableRefObject<Record<string, number>>;
  snapshotNoMoreLinesRef: MutableRefObject<Record<string, boolean>>;
  snapshotLoadingRef: MutableRefObject<boolean>;
  supportsSnapshotPagingRef: MutableRefObject<boolean>;
  snapshotRequestContextRef: MutableRefObject<TerminalSocketSnapshotRequestContext | null>;
}

interface TerminalSocketSetters {
  setConnectionState: Dispatch<SetStateAction<TerminalConnectionState>>;
  setErrorMessage: Dispatch<SetStateAction<string | null>>;
}

export const resetTerminalSocketSnapshotState = ({
  inputForwardEnabledRef,
  appliedSnapshotRef,
  snapshotLoadingRef,
  supportsSnapshotPagingRef,
  snapshotRequestContextRef,
}: Pick<
  TerminalSocketSnapshotStateRefs,
  | 'inputForwardEnabledRef'
  | 'appliedSnapshotRef'
  | 'snapshotLoadingRef'
  | 'supportsSnapshotPagingRef'
  | 'snapshotRequestContextRef'
>): void => {
  inputForwardEnabledRef.current = false;
  appliedSnapshotRef.current = '';
  snapshotLoadingRef.current = false;
  supportsSnapshotPagingRef.current = false;
  snapshotRequestContextRef.current = null;
};

export const resetTerminalSocketConnectionState = ({
  inputForwardEnabledRef,
  snapshotLoadingRef,
  supportsSnapshotPagingRef,
  snapshotRequestContextRef,
}: Pick<
  TerminalSocketSnapshotStateRefs,
  | 'inputForwardEnabledRef'
  | 'snapshotLoadingRef'
  | 'supportsSnapshotPagingRef'
  | 'snapshotRequestContextRef'
>): void => {
  inputForwardEnabledRef.current = false;
  snapshotLoadingRef.current = false;
  supportsSnapshotPagingRef.current = false;
  snapshotRequestContextRef.current = null;
};

export const applyTerminalSocketOpen = ({
  term,
  ws,
  inputForwardEnabledRef,
  setConnectionState,
}: {
  term: XTerm | null;
  ws: WebSocket;
  inputForwardEnabledRef: MutableRefObject<boolean>;
  setConnectionState: Dispatch<SetStateAction<TerminalConnectionState>>;
}): void => {
  setConnectionState('connected');
  if (term) {
    ws.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
  }
  inputForwardEnabledRef.current = true;
};

export const applyTerminalSnapshotMessage = ({
  terminalId,
  snapshot,
  requestContext,
  term,
  outputParseStateRef,
  appendCommands,
  appliedSnapshotRef,
  snapshotVisibleLinesRef,
  snapshotNoMoreLinesRef,
  snapshotLoadingRef,
  snapshotRequestContextRef,
}: {
  terminalId: string;
  snapshot: string;
  requestContext: TerminalSocketSnapshotRequestContext | null;
  term: XTerm | null;
  outputParseStateRef: MutableRefObject<CommandHistoryParseState>;
  appendCommands: AppendCommandsFn;
  appliedSnapshotRef: MutableRefObject<string>;
  snapshotVisibleLinesRef: MutableRefObject<Record<string, number>>;
  snapshotNoMoreLinesRef: MutableRefObject<Record<string, boolean>>;
  snapshotLoadingRef: MutableRefObject<boolean>;
  snapshotRequestContextRef: MutableRefObject<TerminalSocketSnapshotRequestContext | null>;
}): void => {
  const currentSnapshot = appliedSnapshotRef.current;
  const previousLineCount = countSnapshotLines(currentSnapshot);
  const nextLineCount = countSnapshotLines(snapshot);
  const hasMoreLines = nextLineCount > previousLineCount;

  if (snapshot !== currentSnapshot) {
    appliedSnapshotRef.current = snapshot;
    if (term) {
      term.reset();
      term.write(snapshot);
      const parsedSnapshot = parseOutputChunkForCommands(
        snapshot,
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
    snapshotNoMoreLinesRef.current[terminalId] = (
      !hasMoreLines || requestContext.requestedLines >= TERMINAL_SNAPSHOT_MAX_LINES
    );
  } else {
    snapshotVisibleLinesRef.current[terminalId] = Math.max(
      TERMINAL_SNAPSHOT_INITIAL_LINES,
      Math.min(TERMINAL_SNAPSHOT_MAX_LINES, nextLineCount),
    );
    snapshotNoMoreLinesRef.current[terminalId] = false;
  }

  snapshotLoadingRef.current = false;
  snapshotRequestContextRef.current = null;
};

export const applyTerminalOutputMessage = ({
  terminalId,
  outputData,
  term,
  outputParseStateRef,
  replayingHistoryRef,
  pendingOutputChunksRef,
  terminalFirstOutputLoggedRef,
  terminalOpenStartedAtRef,
  appendCommands,
}: {
  terminalId: string;
  outputData: string;
  term: XTerm | null;
  outputParseStateRef: MutableRefObject<CommandHistoryParseState>;
  replayingHistoryRef: MutableRefObject<boolean>;
  pendingOutputChunksRef: MutableRefObject<string[]>;
  terminalFirstOutputLoggedRef: MutableRefObject<boolean>;
  terminalOpenStartedAtRef: MutableRefObject<number | null>;
  appendCommands: AppendCommandsFn;
}): void => {
  if (replayingHistoryRef.current) {
    pendingOutputChunksRef.current.push(outputData);
    return;
  }
  if (
    !terminalFirstOutputLoggedRef.current
    && terminalOpenStartedAtRef.current
    && outputData.length > 0
  ) {
    terminalFirstOutputLoggedRef.current = true;
    debugLog('[Perf] terminal first realtime output', {
      terminalId,
      elapsedMs: Date.now() - terminalOpenStartedAtRef.current,
    });
  }
  term?.write(outputData);

  const parsed = parseOutputChunkForCommands(outputData, outputParseStateRef.current);
  outputParseStateRef.current = parsed.nextState;
  appendCommands(parsed.commands, new Date().toISOString(), 'correct');
};

export const applyTerminalExitMessage = ({
  inputForwardEnabledRef,
  setConnectionState,
}: Pick<TerminalSocketSnapshotStateRefs, 'inputForwardEnabledRef'> & Pick<TerminalSocketSetters, 'setConnectionState'>): void => {
  inputForwardEnabledRef.current = false;
  setConnectionState('disconnected');
};

export const applyTerminalStateMessage = ({
  snapshotPaging,
  supportsSnapshotPagingRef,
}: {
  snapshotPaging: unknown;
  supportsSnapshotPagingRef: MutableRefObject<boolean>;
}): void => {
  if (typeof snapshotPaging === 'boolean') {
    supportsSnapshotPagingRef.current = snapshotPaging;
  }
};

export const applyTerminalErrorMessage = ({
  message,
  inputForwardEnabledRef,
  setConnectionState,
  setErrorMessage,
}: Pick<TerminalSocketSnapshotStateRefs, 'inputForwardEnabledRef'> & Pick<TerminalSocketSetters, 'setConnectionState' | 'setErrorMessage'> & {
  message: string;
}): void => {
  setErrorMessage(message);
  inputForwardEnabledRef.current = false;
  setConnectionState('error');
};
