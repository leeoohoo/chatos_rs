import type { MutableRefObject } from 'react';
import type { FitAddon } from '@xterm/addon-fit';
import type { Terminal as XTerm } from '@xterm/xterm';

import type {
  InputCommandParseState,
} from './commandHistory';
import { extractCommandFromTerminalBuffer } from './commandHistory';
import type { AppendCommandsFn } from './useTerminalAppendCommands';
import { resolveTerminalInputEvent } from './terminalInputState';
import { resolveTerminalSnapshotLoadRequest } from './terminalHistoryState';
import {
  resolveTerminalResizeSendPlan,
  resolveTerminalSnapshotSendPlan,
} from './terminalViewportState';

export interface TerminalSnapshotRequestContext {
  terminalId: string;
  requestedLines: number;
  fromScroll: boolean;
}

export const createTerminalDataHandler = ({
  term,
  socketRef,
  inputForwardEnabledRef,
  inputParseStateRef,
  appendCommands,
}: {
  term: XTerm;
  socketRef: MutableRefObject<WebSocket | null>;
  inputForwardEnabledRef: MutableRefObject<boolean>;
  inputParseStateRef: MutableRefObject<InputCommandParseState>;
  appendCommands: AppendCommandsFn;
}) => (data: string) => {
  if (!inputForwardEnabledRef.current) {
    return;
  }

  const submittedCommand = (data.includes('\r') || data.includes('\n'))
    ? extractCommandFromTerminalBuffer(term)
    : null;
  const resolved = resolveTerminalInputEvent({
    data,
    currentInputState: inputParseStateRef.current,
    submittedCommand,
    socketReadyState: socketRef.current?.readyState ?? null,
  });
  inputParseStateRef.current = resolved.nextInputState;

  const createdAt = new Date().toISOString();
  resolved.appendPlans.forEach((plan) => {
    appendCommands(plan.commands, createdAt, plan.mode);
  });

  const ws = socketRef.current;
  if (!ws) {
    return;
  }
  resolved.socketPlans.forEach((plan) => {
    ws.send(plan.payload);
  });
};

export const createTerminalScrollHandler = ({
  terminalId,
  socketRef,
  snapshotVisibleLinesRef,
  snapshotNoMoreLinesRef,
  snapshotLoadingRef,
  supportsSnapshotPagingRef,
  snapshotRequestContextRef,
  scrollTopLoadThreshold,
  initialLines,
  maxLines,
  pageLines,
}: {
  terminalId: string;
  socketRef: MutableRefObject<WebSocket | null>;
  snapshotVisibleLinesRef: MutableRefObject<Record<string, number>>;
  snapshotNoMoreLinesRef: MutableRefObject<Record<string, boolean>>;
  snapshotLoadingRef: MutableRefObject<boolean>;
  supportsSnapshotPagingRef: MutableRefObject<boolean>;
  snapshotRequestContextRef: MutableRefObject<TerminalSnapshotRequestContext | null>;
  scrollTopLoadThreshold: number;
  initialLines: number;
  maxLines: number;
  pageLines: number;
}) => (viewportY: number) => {
  const request = resolveTerminalSnapshotLoadRequest({
    viewportY,
    terminalId,
    supportsSnapshotPaging: supportsSnapshotPagingRef.current,
    snapshotLoading: snapshotLoadingRef.current,
    noMoreLines: snapshotNoMoreLinesRef.current[terminalId] === true,
    currentLines: snapshotVisibleLinesRef.current[terminalId] ?? initialLines,
    scrollTopLoadThreshold,
    initialLines,
    maxLines,
    pageLines,
    socketReadyState: socketRef.current?.readyState ?? null,
  });
  const snapshotPlan = resolveTerminalSnapshotSendPlan({
    shouldRequest: request.shouldRequest,
    reachedEnd: request.reachedEnd,
    nextLines: request.nextLines,
    requestContext: request.requestContext,
    hasSocket: Boolean(socketRef.current),
  });

  if (snapshotPlan.shouldMarkNoMoreLines) {
    snapshotNoMoreLinesRef.current[terminalId] = true;
    return;
  }
  if (!snapshotPlan.shouldRequestSnapshot) {
    return;
  }

  const ws = socketRef.current;
  if (!ws || !snapshotPlan.snapshotPayload) {
    return;
  }
  snapshotLoadingRef.current = snapshotPlan.nextSnapshotLoading;
  snapshotRequestContextRef.current = snapshotPlan.requestContext;
  ws.send(snapshotPlan.snapshotPayload);
};

export const createTerminalResizeObserverHandler = ({
  fitRef,
  terminalRef,
  socketRef,
}: {
  fitRef: MutableRefObject<FitAddon | null>;
  terminalRef: MutableRefObject<XTerm | null>;
  socketRef: MutableRefObject<WebSocket | null>;
}) => () => {
  const resizePlan = resolveTerminalResizeSendPlan({
    hasFitAddon: Boolean(fitRef.current),
    socketReadyState: socketRef.current?.readyState ?? null,
    cols: terminalRef.current?.cols ?? null,
    rows: terminalRef.current?.rows ?? null,
  });
  if (!resizePlan.shouldFit) {
    return;
  }
  fitRef.current?.fit();
  const active = socketRef.current;
  if (active && resizePlan.resizePayload) {
    active.send(resizePlan.resizePayload);
  }
};

export const resetTerminalInputParseState = (
  inputParseStateRef: MutableRefObject<InputCommandParseState>,
) => {
  inputParseStateRef.current = {
    lineBuffer: '',
    skipFollowingLf: false,
  };
};
