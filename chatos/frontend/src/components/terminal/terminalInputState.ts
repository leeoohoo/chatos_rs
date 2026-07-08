// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { InputCommandParseState } from './commandHistory';
import {
  canCommandBeUsed,
  normalizeCommandForCompare,
  parseInputChunkForCommands,
} from './commandHistory';

interface CommandAppendPlan {
  commands: string[];
  mode: 'append' | 'correct';
}

interface TerminalSocketMessagePlan {
  type: 'command' | 'input';
  payload: string;
}

export interface TerminalInputResolution {
  nextInputState: InputCommandParseState;
  appendPlans: CommandAppendPlan[];
  socketPlans: TerminalSocketMessagePlan[];
  normalizedSubmittedCommand: string;
}

const OPEN_WEBSOCKET_READY_STATE = 1;

export const resolveTerminalInputEvent = ({
  data,
  currentInputState,
  submittedCommand,
  socketReadyState,
}: {
  data: string;
  currentInputState: InputCommandParseState;
  submittedCommand: string | null;
  socketReadyState: number | null;
}): TerminalInputResolution => {
  const parsedInput = parseInputChunkForCommands(data, currentInputState);
  const normalizedSubmittedCommand = submittedCommand
    ? normalizeCommandForCompare(submittedCommand)
    : '';

  const appendPlans: CommandAppendPlan[] = [];
  if (parsedInput.commands.length > 0) {
    appendPlans.push({
      commands: parsedInput.commands,
      mode: 'append',
    });
  }
  if (canCommandBeUsed(normalizedSubmittedCommand)) {
    appendPlans.push({
      commands: [normalizedSubmittedCommand],
      mode: 'correct',
    });
  }

  const socketPlans: TerminalSocketMessagePlan[] = [];
  if (socketReadyState === OPEN_WEBSOCKET_READY_STATE) {
    if (canCommandBeUsed(normalizedSubmittedCommand)) {
      socketPlans.push({
        type: 'command',
        payload: JSON.stringify({ type: 'command', command: normalizedSubmittedCommand }),
      });
    }
    socketPlans.push({
      type: 'input',
      payload: JSON.stringify({ type: 'input', data }),
    });
  }

  return {
    nextInputState: parsedInput.nextState,
    appendPlans,
    socketPlans,
    normalizedSubmittedCommand,
  };
};
