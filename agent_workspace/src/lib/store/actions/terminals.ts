import type { Terminal } from '../../../types';
import type ApiClient from '../../api/client';
import { ApiRequestError } from '../../api/client/shared';
import { areTerminalListsEqual, areTerminalsEqual, normalizeTerminal } from '../helpers/terminals';

interface Deps {
  set: any;
  get: any;
  client: ApiClient;
  getUserIdParam: () => string;
}

export function createTerminalActions({ set, get, client, getUserIdParam }: Deps) {
  return {
    loadTerminals: async () => {
      try {
        const uid = getUserIdParam();
        const list = await client.listTerminals(uid);
        const formatted = Array.isArray(list) ? list.map(normalizeTerminal) : [];
        const currentState = get();
        const existingTerminals = Array.isArray(currentState.terminals) ? currentState.terminals : [];
        const rememberedTerminalId = localStorage.getItem(`lastTerminalId_${uid}`);
        const currentTerminalId = typeof currentState.currentTerminalId === 'string'
          ? currentState.currentTerminalId.trim()
          : '';
        const nextCurrentTerminalId = (
          currentTerminalId && formatted.some((terminal) => terminal.id === currentTerminalId)
            ? currentTerminalId
            : ''
        ) || (
          rememberedTerminalId
            ? (formatted.find((terminal) => terminal.id === rememberedTerminalId)?.id || '')
            : ''
        ) || null;
        const nextCurrentTerminal = nextCurrentTerminalId
          ? formatted.find(t => t.id === nextCurrentTerminalId) || null
          : null;

        if (
          areTerminalListsEqual(existingTerminals, formatted)
          && currentState.currentTerminalId === nextCurrentTerminalId
          && areTerminalsEqual(currentState.currentTerminal, nextCurrentTerminal)
        ) {
          return formatted;
        }

        set((state: any) => {
          state.terminals = formatted;
          state.currentTerminalId = nextCurrentTerminalId;
          state.currentTerminal = nextCurrentTerminal;
          if (!nextCurrentTerminalId) {
            localStorage.removeItem(`lastTerminalId_${uid}`);
          }
        });
        return formatted;
      } catch (error) {
        console.error('Failed to load terminals:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to load terminals';
        });
        return [];
      }
    },

    createTerminal: async (cwd: string, name?: string) => {
      const uid = getUserIdParam();
      const payload = {
        name: name?.trim() || undefined,
        cwd,
        user_id: uid,
      };
      const created = await client.createTerminal(payload);
      const terminal = normalizeTerminal(created);
      set((state: any) => {
        state.terminals.unshift(terminal);
        state.currentTerminalId = terminal.id;
        state.currentTerminal = terminal;
        state.activePanel = 'terminal';
      });
      localStorage.setItem(`lastTerminalId_${uid}`, terminal.id);
      return terminal;
    },

    deleteTerminal: async (terminalId: string) => {
      try {
        await client.deleteTerminal(terminalId);
        set((state: any) => {
          state.terminals = state.terminals.filter((t: any) => t.id !== terminalId);
          if (state.currentTerminalId === terminalId) {
            state.currentTerminalId = null;
            state.currentTerminal = null;
            if (state.activePanel === 'terminal') {
              state.activePanel = 'chat';
            }
          }
        });
      } catch (error) {
        console.error('Failed to delete terminal:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete terminal';
        });
      }
    },

    selectTerminal: async (terminalId: string) => {
      try {
        let terminal = get().terminals.find((t: Terminal) => t.id === terminalId) || null;
        if (!terminal) {
          const fetched = await client.getTerminal(terminalId);
          terminal = normalizeTerminal(fetched);
        }
        const uid = getUserIdParam();
        set((state: any) => {
          state.currentTerminalId = terminalId;
          state.currentTerminal = terminal;
          state.activePanel = 'terminal';
        });
        localStorage.setItem(`lastTerminalId_${uid}`, terminalId);
      } catch (error) {
        const uid = getUserIdParam();
        if (error instanceof ApiRequestError && error.status === 404) {
          localStorage.removeItem(`lastTerminalId_${uid}`);
          const refreshed = await get().loadTerminals();
          const fallback = Array.isArray(refreshed) && refreshed.length > 0
            ? refreshed[0]
            : null;
          set((state: any) => {
            state.currentTerminalId = fallback?.id || null;
            state.currentTerminal = fallback || null;
            if (fallback) {
              state.activePanel = 'terminal';
            }
          });
          return;
        }
        console.error('Failed to select terminal:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to select terminal';
        });
      }
    },

  };
}
