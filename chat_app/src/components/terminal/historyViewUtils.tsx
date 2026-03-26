import React from 'react';
import type { TerminalLog } from '../../types';
import {
  canCommandBeUsed,
  canOutputCommandCorrectInput,
  createInitialCommandHistoryParseState,
  createInitialInputCommandParseState,
  normalizeCommandForCompare,
  normalizeLogTimestamp,
  parseInputChunkForCommands,
  parseOutputChunkForCommands,
  type CommandHistoryItem,
  type CommandHistoryParseState,
} from './commandHistory';

export const TERMINAL_HISTORY_INITIAL_LIMIT = 120;
export const TERMINAL_HISTORY_PAGE_SIZE = 600;
export const TERMINAL_HISTORY_MAX_LIMIT = 3000;
export const TERMINAL_HISTORY_TAIL_ONLY_HINT = '已预载更早历史，终端窗口保持实时 tail 模式以确保流畅。';
export const TERMINAL_SNAPSHOT_INITIAL_LINES = 500;
export const TERMINAL_SNAPSHOT_PAGE_LINES = 500;
export const TERMINAL_SNAPSHOT_MAX_LINES = 10_000;
export const TERMINAL_SCROLL_TOP_LOAD_THRESHOLD = 0;

export const closeWebSocketSafely = (socket: WebSocket | null | undefined) => {
  if (!socket) {
    return;
  }
  if (socket.readyState === WebSocket.OPEN) {
    socket.close();
    return;
  }
  if (socket.readyState === WebSocket.CONNECTING) {
    const closeOnOpen = () => {
      try {
        socket.close();
      } catch {
        // ignore
      }
    };
    socket.addEventListener('open', closeOnOpen, { once: true });
  }
};

export const countSnapshotLines = (snapshot: string): number => {
  if (!snapshot) {
    return 0;
  }
  return snapshot.split('\n').length;
};

const COMMAND_OPERATORS = new Set([
  '|',
  '||',
  '&&',
  ';',
  '>',
  '>>',
  '<',
  '<<',
  '2>',
  '2>>',
]);

const COMMAND_TOKEN_REGEX = /(\s+|\|\||&&|2>>|2>|>>|<<|\||;|>|<|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`|\S+)/g;

const isEnvAssignmentToken = (token: string): boolean => (
  /^[A-Za-z_][A-Za-z0-9_]*=.*/.test(token)
);

const isQuotedToken = (token: string): boolean => (
  (token.startsWith('"') && token.endsWith('"'))
  || (token.startsWith('\'') && token.endsWith('\''))
  || (token.startsWith('`') && token.endsWith('`'))
);

const isPathLikeToken = (token: string): boolean => (
  token.startsWith('./')
  || token.startsWith('../')
  || token.startsWith('~/')
  || token.startsWith('/')
  || token.includes('/')
  || token.includes('\\')
);

export const renderHighlightedCommand = (command: string): React.ReactNode => {
  if (!command) {
    return '';
  }

  const tokens = command.match(COMMAND_TOKEN_REGEX) || [command];
  let commandMarked = false;
  let expectsCommand = true;

  return tokens.map((token, index) => {
    if (/^\s+$/.test(token)) {
      return <span key={`cmd-token-${index}`}>{token}</span>;
    }

    let className = 'text-foreground';

    if (COMMAND_OPERATORS.has(token)) {
      className = 'text-fuchsia-500';
      expectsCommand = true;
    } else if (expectsCommand && isEnvAssignmentToken(token)) {
      className = 'text-cyan-500';
    } else if (!commandMarked) {
      className = 'font-semibold text-sky-500';
      commandMarked = true;
      expectsCommand = false;
    } else if (token.startsWith('-')) {
      className = 'text-amber-500';
    } else if (isQuotedToken(token)) {
      className = 'text-emerald-500';
    } else if (isPathLikeToken(token)) {
      className = 'text-green-500';
    }

    return (
      <span key={`cmd-token-${index}`} className={className}>
        {token}
      </span>
    );
  });
};

type ParsedCommandHistory = {
  commands: CommandHistoryItem[];
  outputState: CommandHistoryParseState;
  nextSequence: number;
  outputLogs: TerminalLog[];
};

export const parseCommandHistoryFromLogs = (
  logs: TerminalLog[],
  startSequence: number,
): ParsedCommandHistory => {
  const outputLogs: TerminalLog[] = [];
  const commandLogs: TerminalLog[] = [];
  const inputLogs: TerminalLog[] = [];

  for (const log of logs) {
    if (log.logType === 'command') {
      commandLogs.push(log);
      continue;
    }
    if (log.logType === 'input') {
      inputLogs.push(log);
      continue;
    }
    if (log.logType === 'output' || log.logType === 'system') {
      outputLogs.push(log);
    }
  }

  let seq = startSequence;
  const parsedCommands: CommandHistoryItem[] = [];

  if (commandLogs.length > 0) {
    for (const log of commandLogs) {
      const normalizedCommand = normalizeCommandForCompare(log.content);
      if (!canCommandBeUsed(normalizedCommand)) {
        continue;
      }
      parsedCommands.push({
        id: `cmd-${seq++}`,
        command: normalizedCommand,
        createdAt: normalizeLogTimestamp(log.createdAt),
      });
    }
  }

  let inputState = createInitialInputCommandParseState();

  if (parsedCommands.length === 0) {
    for (const log of inputLogs) {
      const parsed = parseInputChunkForCommands(log.content, inputState);
      inputState = parsed.nextState;
      if (parsed.commands.length === 0) {
        continue;
      }

      const createdAt = normalizeLogTimestamp(log.createdAt);
      for (const command of parsed.commands) {
        const normalizedCommand = normalizeCommandForCompare(command);
        if (!canCommandBeUsed(normalizedCommand)) {
          continue;
        }

        parsedCommands.push({
          id: `cmd-${seq++}`,
          command: normalizedCommand,
          createdAt,
        });
      }
    }
  }

  const outputDerivedCommands: CommandHistoryItem[] = [];
  let outputState = createInitialCommandHistoryParseState();

  for (const log of outputLogs) {
    const parsed = parseOutputChunkForCommands(log.content, outputState);
    outputState = parsed.nextState;

    if (commandLogs.length > 0 || parsed.commands.length === 0) {
      continue;
    }

    const createdAt = normalizeLogTimestamp(log.createdAt);
    for (const command of parsed.commands) {
      const normalizedOutputCommand = normalizeCommandForCompare(command);
      if (!canCommandBeUsed(normalizedOutputCommand)) {
        continue;
      }

      outputDerivedCommands.push({
        id: `cmd-${seq++}`,
        command: normalizedOutputCommand,
        createdAt,
      });

      if (parsedCommands.length === 0) {
        continue;
      }

      const searchStart = Math.max(0, parsedCommands.length - 10);
      for (let i = parsedCommands.length - 1; i >= searchStart; i -= 1) {
        const baseCommand = parsedCommands[i].command;
        if (canOutputCommandCorrectInput(baseCommand, normalizedOutputCommand)
          && normalizedOutputCommand.length > normalizeCommandForCompare(baseCommand).length
        ) {
          parsedCommands[i] = {
            ...parsedCommands[i],
            command: normalizedOutputCommand,
            createdAt,
          };
          break;
        }
      }
    }
  }

  if (commandLogs.length === 0 && parsedCommands.length === 0 && outputDerivedCommands.length > 0) {
    parsedCommands.push(...outputDerivedCommands);
  }

  return {
    commands: parsedCommands,
    outputState,
    nextSequence: seq,
    outputLogs,
  };
};
