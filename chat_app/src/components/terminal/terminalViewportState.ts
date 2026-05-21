export interface TerminalSnapshotSendPlan {
  shouldMarkNoMoreLines: boolean;
  shouldRequestSnapshot: boolean;
  nextSnapshotLoading: boolean;
  requestContext: {
    terminalId: string;
    requestedLines: number;
    fromScroll: boolean;
  } | null;
  snapshotPayload: string | null;
}

export interface TerminalResizeSendPlan {
  shouldFit: boolean;
  resizePayload: string | null;
}

const OPEN_WEBSOCKET_READY_STATE = 1;

export const resolveTerminalSnapshotSendPlan = ({
  shouldRequest,
  reachedEnd,
  nextLines,
  requestContext,
  hasSocket,
}: {
  shouldRequest: boolean;
  reachedEnd: boolean;
  nextLines: number;
  requestContext: {
    terminalId: string;
    requestedLines: number;
    fromScroll: boolean;
  } | null;
  hasSocket: boolean;
}): TerminalSnapshotSendPlan => {
  if (reachedEnd) {
    return {
      shouldMarkNoMoreLines: true,
      shouldRequestSnapshot: false,
      nextSnapshotLoading: false,
      requestContext: null,
      snapshotPayload: null,
    };
  }

  if (!shouldRequest || !hasSocket || !requestContext) {
    return {
      shouldMarkNoMoreLines: false,
      shouldRequestSnapshot: false,
      nextSnapshotLoading: false,
      requestContext: null,
      snapshotPayload: null,
    };
  }

  return {
    shouldMarkNoMoreLines: false,
    shouldRequestSnapshot: true,
    nextSnapshotLoading: true,
    requestContext,
    snapshotPayload: JSON.stringify({ type: 'snapshot', lines: nextLines }),
  };
};

export const resolveTerminalResizeSendPlan = ({
  hasFitAddon,
  socketReadyState,
  cols,
  rows,
}: {
  hasFitAddon: boolean;
  socketReadyState: number | null;
  cols: number | null;
  rows: number | null;
}): TerminalResizeSendPlan => {
  if (!hasFitAddon) {
    return {
      shouldFit: false,
      resizePayload: null,
    };
  }

  if (
    socketReadyState !== OPEN_WEBSOCKET_READY_STATE
    || cols === null
    || rows === null
  ) {
    return {
      shouldFit: true,
      resizePayload: null,
    };
  }

  return {
    shouldFit: true,
    resizePayload: JSON.stringify({ type: 'resize', cols, rows }),
  };
};
