import { useCallback } from 'react';
import type { Dispatch, MutableRefObject, SetStateAction } from 'react';

import type { CommandHistoryItem } from './commandHistory';
import {
  MAX_COMMAND_HISTORY,
  canCommandBeUsed,
  canOutputCommandCorrectInput,
  normalizeCommandForCompare,
} from './commandHistory';

export type AppendCommandsFn = (
  commands: string[],
  createdAt: string,
  mode?: 'append' | 'correct',
) => void;

interface UseTerminalAppendCommandsParams {
  currentTerminalId?: string | null;
  setCommandHistory: Dispatch<SetStateAction<CommandHistoryItem[]>>;
  commandHistoryCacheRef: MutableRefObject<Record<string, CommandHistoryItem[]>>;
  commandSeqRef: MutableRefObject<number>;
}

export const useTerminalAppendCommands = ({
  currentTerminalId,
  setCommandHistory,
  commandHistoryCacheRef,
  commandSeqRef,
}: UseTerminalAppendCommandsParams): AppendCommandsFn => {
  return useCallback((commands: string[], createdAt: string, mode: 'append' | 'correct' = 'append') => {
    if (commands.length === 0) {
      return;
    }

    setCommandHistory((prev) => {
      const next = [...prev];
      const normalizedCommands = commands
        .map((command) => normalizeCommandForCompare(command))
        .filter((command) => canCommandBeUsed(command));

      if (normalizedCommands.length === 0) {
        return next;
      }

      if (mode === 'append') {
        for (const command of normalizedCommands) {
          const last = next[next.length - 1];
          if (last && normalizeCommandForCompare(last.command) === command) {
            continue;
          }

          next.push({
            id: `cmd-${commandSeqRef.current++}`,
            command,
            createdAt,
          });
        }

        const finalHistory = next.slice(-MAX_COMMAND_HISTORY);
        if (currentTerminalId) {
          commandHistoryCacheRef.current[currentTerminalId] = finalHistory;
        }
        return finalHistory;
      }

      if (next.length === 0) {
        return next;
      }

      const windowStart = Math.max(0, next.length - 6);
      for (const outputCommand of normalizedCommands) {
        for (let i = next.length - 1; i >= windowStart; i -= 1) {
          const existing = next[i];
          const existingNormalized = normalizeCommandForCompare(existing.command);

          if (existingNormalized === outputCommand) {
            break;
          }

          if (canOutputCommandCorrectInput(existingNormalized, outputCommand) && outputCommand.length > existingNormalized.length) {
            next[i] = {
              ...existing,
              command: outputCommand,
              createdAt,
            };
            break;
          }
        }
      }

      const finalHistory = next.slice(-MAX_COMMAND_HISTORY);
      if (currentTerminalId) {
        commandHistoryCacheRef.current[currentTerminalId] = finalHistory;
      }
      return finalHistory;
    });
  }, [commandHistoryCacheRef, commandSeqRef, currentTerminalId, setCommandHistory]);
};
