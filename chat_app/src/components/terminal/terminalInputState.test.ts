import { describe, expect, it } from 'vitest';

import { resolveTerminalInputEvent } from './terminalInputState';

describe('terminalInputState', () => {
  it('parses input commands and only sends raw input when no submitted command is available', () => {
    const resolved = resolveTerminalInputEvent({
      data: 'ls',
      currentInputState: {
        lineBuffer: '',
        skipFollowingLf: false,
      },
      submittedCommand: null,
      socketReadyState: 1,
    });

    expect(resolved.nextInputState).toEqual({
      lineBuffer: 'ls',
      skipFollowingLf: false,
    });
    expect(resolved.appendPlans).toEqual([]);
    expect(resolved.socketPlans).toEqual([
      {
        type: 'input',
        payload: JSON.stringify({ type: 'input', data: 'ls' }),
      },
    ]);
  });

  it('corrects submitted commands and sends both command and input payloads when socket is open', () => {
    const resolved = resolveTerminalInputEvent({
      data: '\r',
      currentInputState: {
        lineBuffer: 'git   status',
        skipFollowingLf: false,
      },
      submittedCommand: 'git   status',
      socketReadyState: 1,
    });

    expect(resolved.nextInputState).toEqual({
      lineBuffer: '',
      skipFollowingLf: true,
    });
    expect(resolved.appendPlans).toEqual([
      {
        commands: ['git   status'],
        mode: 'append',
      },
      {
        commands: ['git status'],
        mode: 'correct',
      },
    ]);
    expect(resolved.socketPlans).toEqual([
      {
        type: 'command',
        payload: JSON.stringify({ type: 'command', command: 'git status' }),
      },
      {
        type: 'input',
        payload: JSON.stringify({ type: 'input', data: '\r' }),
      },
    ]);
  });

  it('does not send websocket payloads when the socket is not open', () => {
    const resolved = resolveTerminalInputEvent({
      data: '\r',
      currentInputState: {
        lineBuffer: 'pwd',
        skipFollowingLf: false,
      },
      submittedCommand: 'pwd',
      socketReadyState: 3,
    });

    expect(resolved.appendPlans).toEqual([
      {
        commands: ['pwd'],
        mode: 'append',
      },
      {
        commands: ['pwd'],
        mode: 'correct',
      },
    ]);
    expect(resolved.socketPlans).toEqual([]);
  });
});
