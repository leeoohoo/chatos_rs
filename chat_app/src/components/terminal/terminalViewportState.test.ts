import { describe, expect, it } from 'vitest';

import {
  resolveTerminalResizeSendPlan,
  resolveTerminalSnapshotSendPlan,
} from './terminalViewportState';

describe('terminalViewportState', () => {
  it('marks snapshot exhaustion without sending a request when the scroll range is exhausted', () => {
    const plan = resolveTerminalSnapshotSendPlan({
      shouldRequest: false,
      reachedEnd: true,
      nextLines: 10000,
      requestContext: null,
      hasSocket: true,
    });

    expect(plan).toEqual({
      shouldMarkNoMoreLines: true,
      shouldRequestSnapshot: false,
      nextSnapshotLoading: false,
      requestContext: null,
      snapshotPayload: null,
    });
  });

  it('sends snapshot requests only when a socket and request context are both available', () => {
    const noSocketPlan = resolveTerminalSnapshotSendPlan({
      shouldRequest: true,
      reachedEnd: false,
      nextLines: 1000,
      requestContext: {
        terminalId: 'terminal_1',
        requestedLines: 1000,
        fromScroll: true,
      },
      hasSocket: false,
    });
    expect(noSocketPlan.shouldRequestSnapshot).toBe(false);

    const requestPlan = resolveTerminalSnapshotSendPlan({
      shouldRequest: true,
      reachedEnd: false,
      nextLines: 1000,
      requestContext: {
        terminalId: 'terminal_1',
        requestedLines: 1000,
        fromScroll: true,
      },
      hasSocket: true,
    });
    expect(requestPlan.shouldRequestSnapshot).toBe(true);
    expect(requestPlan.nextSnapshotLoading).toBe(true);
    expect(requestPlan.snapshotPayload).toBe(JSON.stringify({ type: 'snapshot', lines: 1000 }));
  });

  it('fits locally on resize and only sends resize payloads when the socket is open', () => {
    const localOnlyPlan = resolveTerminalResizeSendPlan({
      hasFitAddon: true,
      socketReadyState: 3,
      cols: 120,
      rows: 32,
    });
    expect(localOnlyPlan).toEqual({
      shouldFit: true,
      resizePayload: null,
    });

    const sendPlan = resolveTerminalResizeSendPlan({
      hasFitAddon: true,
      socketReadyState: 1,
      cols: 120,
      rows: 32,
    });
    expect(sendPlan).toEqual({
      shouldFit: true,
      resizePayload: JSON.stringify({ type: 'resize', cols: 120, rows: 32 }),
    });
  });
});
