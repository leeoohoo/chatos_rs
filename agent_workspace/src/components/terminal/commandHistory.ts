import type { Terminal as XTerm } from '@xterm/xterm';

export interface CommandHistoryItem {
  id: string;
  command: string;
  createdAt: string;
}

export interface CommandHistoryParseState {
  lineBuffer: string;
}

export interface InputCommandParseState {
  lineBuffer: string;
  skipFollowingLf: boolean;
}

export const MAX_COMMAND_HISTORY = 200;

export const createInitialCommandHistoryParseState = (): CommandHistoryParseState => ({
  lineBuffer: '',
});

export const createInitialInputCommandParseState = (): InputCommandParseState => ({
  lineBuffer: '',
  skipFollowingLf: false,
});

const stripTerminalControlSequences = (input: string): string => (
  input
    .replace(/\u001b\][^\u001b\u0007]*(?:\u0007|\u001b\\)/g, '')
    .replace(/\u001b\[[0-?]*[ -/]*[@-~]/g, '')
    .replace(/\u001b[@-Z\\-_]/g, '')
);

const collapseBackspaces = (input: string): string => {
  let out = '';
  for (const ch of input) {
    if (ch === '\u0008' || ch === '\u007f') {
      out = out.slice(0, -1);
      continue;
    }
    if (ch === '\u0000') {
      continue;
    }
    out += ch;
  }
  return out;
};

const looksLikePromptPrefix = (prefixWithMarker: string): boolean => {
  const marker = prefixWithMarker[prefixWithMarker.length - 1];
  if (!marker || !['$', '%', '#', '>'].includes(marker)) {
    return false;
  }

  const prefix = prefixWithMarker.slice(0, -1).trim();
  if (!prefix) {
    return false;
  }

  if (prefix.startsWith('(') || prefix.includes('@')) {
    return true;
  }

  if (
    prefix.startsWith('~')
    || prefix.startsWith('/')
    || prefix.startsWith('./')
    || prefix.startsWith('../')
  ) {
    return true;
  }

  return /^[A-Za-z]:/.test(prefix) && prefix.charAt(2) === '\\';
};

const getVisibleLineAfterCarriageReturn = (line: string): string => {
  const withoutTrailingCr = line.replace(/\r+$/, '');
  const segments = withoutTrailingCr.split('\r');
  return segments[segments.length - 1] ?? '';
};

const extractCommandFromPromptLine = (line: string): string | null => {
  const visible = collapseBackspaces(getVisibleLineAfterCarriageReturn(line)).trimEnd();
  if (!visible) {
    return null;
  }

  const normalize = (value: string): string => value.replace(/\u0007/g, '').trim();

  const windowsPrompt = visible.match(/^([A-Za-z]:\\\\.*>)\s*(.+)$/);
  if (windowsPrompt?.[2]) {
    const command = normalize(windowsPrompt[2]);
    return command.length > 0 && command.length <= 300 ? command : null;
  }

  const unixPromptWithSpace = visible.match(/^(.*\s[#$%>])\s+(.+)$/);
  if (unixPromptWithSpace?.[2] && looksLikePromptPrefix(unixPromptWithSpace[1])) {
    const command = normalize(unixPromptWithSpace[2]);
    return command.length > 0 && command.length <= 300 ? command : null;
  }

  const unixUserHostPrompt = visible.match(/^([^\r\n]*@[^\r\n]*[#$%>])\s+(.+)$/);
  if (unixUserHostPrompt?.[2]) {
    const command = normalize(unixUserHostPrompt[2]);
    return command.length > 0 && command.length <= 300 ? command : null;
  }

  return null;
};

export const parseOutputChunkForCommands = (
  chunk: string,
  state: CommandHistoryParseState,
): { commands: string[]; nextState: CommandHistoryParseState } => {
  const cleaned = stripTerminalControlSequences(chunk);
  const combined = `${state.lineBuffer}${cleaned}`;
  const lines = combined.split('\n');
  const nextLineBuffer = lines.pop() ?? '';
  const commands: string[] = [];

  for (const rawLine of lines) {
    const command = extractCommandFromPromptLine(rawLine);
    if (command) {
      commands.push(command);
    }
  }

  return {
    commands,
    nextState: {
      lineBuffer: nextLineBuffer,
    },
  };
};

export const parseInputChunkForCommands = (
  chunk: string,
  state: InputCommandParseState,
): { commands: string[]; nextState: InputCommandParseState } => {
  const commands: string[] = [];
  let lineBuffer = state.lineBuffer;
  let skipFollowingLf = state.skipFollowingLf;
  const cleaned = stripTerminalControlSequences(chunk);

  for (const ch of cleaned) {
    if (skipFollowingLf && ch !== '\n') {
      skipFollowingLf = false;
    }

    if (ch === '\r' || ch === '\n') {
      if (skipFollowingLf && ch === '\n') {
        skipFollowingLf = false;
        continue;
      }

      const command = lineBuffer.trim();
      if (command) {
        commands.push(command);
      }
      lineBuffer = '';
      skipFollowingLf = ch === '\r';
      continue;
    }

    if (ch === '\u0008' || ch === '\u007f') {
      lineBuffer = lineBuffer.slice(0, -1);
      continue;
    }

    if (ch === '\u0015' || ch === '\u0003' || ch === '\u0004' || ch === '\u001a') {
      lineBuffer = '';
      continue;
    }

    if (ch < ' ' || ch === '\u007f') {
      continue;
    }

    lineBuffer += ch;
  }

  return {
    commands,
    nextState: {
      lineBuffer,
      skipFollowingLf,
    },
  };
};

const getCurrentPromptLineFromTerminalBuffer = (term: XTerm): string => {
  const buffer = term.buffer.active;
  let y = buffer.cursorY;
  const parts: string[] = [];

  while (y >= 0) {
    const line = buffer.getLine(y);
    if (!line) {
      break;
    }

    parts.unshift(line.translateToString(true));
    if (!line.isWrapped) {
      break;
    }

    y -= 1;
  }

  return parts.join('');
};

export const extractCommandFromTerminalBuffer = (term: XTerm): string | null => {
  const promptLine = getCurrentPromptLineFromTerminalBuffer(term);
  return extractCommandFromPromptLine(promptLine);
};

export const normalizeCommandForCompare = (command: string): string => (
  command.replace(/\s+/g, ' ').replace(/\u0007/g, '').trim()
);

export const canCommandBeUsed = (command: string): boolean => {
  const normalized = normalizeCommandForCompare(command);
  return normalized.length > 0 && normalized.length <= 300;
};

export const canOutputCommandCorrectInput = (inputCommand: string, outputCommand: string): boolean => {
  const inputNormalized = normalizeCommandForCompare(inputCommand);
  const outputNormalized = normalizeCommandForCompare(outputCommand);

  if (!inputNormalized || !outputNormalized) {
    return false;
  }

  return outputNormalized.startsWith(inputNormalized) || inputNormalized.startsWith(outputNormalized);
};

export const normalizeLogTimestamp = (value: Date | string | undefined): string => {
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (typeof value === 'string' && value.trim()) {
    const parsed = new Date(value);
    if (!Number.isNaN(parsed.getTime())) {
      return parsed.toISOString();
    }
  }
  return new Date().toISOString();
};

export const formatCommandTime = (value: string): string => {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return '--:--:--';
  }
  return parsed.toLocaleTimeString('zh-CN', {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
};

const getTimestampMs = (value: string): number => {
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? 0 : parsed;
};

const sortCommandHistory = (items: CommandHistoryItem[]): CommandHistoryItem[] => (
  [...items].sort((a, b) => getTimestampMs(a.createdAt) - getTimestampMs(b.createdAt))
);

export const mergeCommandHistory = (
  logHistory: CommandHistoryItem[],
  cachedHistory: CommandHistoryItem[],
): CommandHistoryItem[] => {
  const dedup = new Map<string, CommandHistoryItem>();
  for (const item of [...logHistory, ...cachedHistory]) {
    const normalizedCommand = normalizeCommandForCompare(item.command);
    if (!canCommandBeUsed(normalizedCommand)) {
      continue;
    }
    const createdAt = normalizeLogTimestamp(item.createdAt);
    const key = `${normalizedCommand}@@${createdAt}`;
    if (dedup.has(key)) {
      continue;
    }
    dedup.set(key, {
      ...item,
      command: normalizedCommand,
      createdAt,
    });
  }
  return sortCommandHistory(Array.from(dedup.values())).slice(-MAX_COMMAND_HISTORY);
};

export const writeToTerminal = (term: XTerm, data: string): Promise<void> => (
  new Promise((resolve) => {
    if (!data) {
      resolve();
      return;
    }
    term.write(data, () => resolve());
  })
);

export const writeToTerminalInChunks = async (
  term: XTerm,
  data: string,
  chunkSize = 24 * 1024,
): Promise<void> => {
  if (!data) {
    return;
  }
  for (let i = 0; i < data.length; i += chunkSize) {
    const chunk = data.slice(i, i + chunkSize);
    await writeToTerminal(term, chunk);
  }
};
