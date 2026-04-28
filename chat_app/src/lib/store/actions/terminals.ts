import type { Terminal } from '../../../types';
import type ApiClient from '../../api/client';
import { ApiRequestError } from '../../api/client/shared';
import { normalizeTerminal } from '../helpers/terminals';
import type { ChatStoreDraft, ChatStoreGet, ChatStoreSet } from '../types';

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
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
        set((state: ChatStoreDraft) => {
          state.terminals = formatted;
          if (!state.currentTerminalId) {
            const lastId = localStorage.getItem(`lastTerminalId_${uid}`);
            if (lastId) {
              const matched = formatted.find(t => t.id === lastId);
              if (matched) {
                state.currentTerminalId = matched.id;
                state.currentTerminal = matched;
              }
            }
          } else {
            const matched = formatted.find(t => t.id === state.currentTerminalId);
            if (matched) {
              state.currentTerminal = matched;
            } else {
              state.currentTerminalId = null;
              state.currentTerminal = null;
              if (state.activePanel === 'terminal') {
                state.activePanel = 'chat';
              }
            }
          }
        });
        return formatted;
      } catch (error) {
        console.error('Failed to load terminals:', error);
        set((state: ChatStoreDraft) => {
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
      set((state: ChatStoreDraft) => {
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
        set((state: ChatStoreDraft) => {
          state.terminals = state.terminals.filter((terminal) => terminal.id !== terminalId);
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
        set((state: ChatStoreDraft) => {
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
        set((state: ChatStoreDraft) => {
          state.currentTerminalId = terminalId;
          state.currentTerminal = terminal;
          state.activePanel = 'terminal';
        });
        localStorage.setItem(`lastTerminalId_${uid}`, terminalId);
      } catch (error) {
        console.error('Failed to select terminal:', error);
        set((state: ChatStoreDraft) => {
          if (error instanceof ApiRequestError && error.status === 404 && state.currentTerminalId === terminalId) {
            state.currentTerminalId = null;
            state.currentTerminal = null;
            if (state.activePanel === 'terminal') {
              state.activePanel = 'chat';
            }
          }
          state.error = error instanceof Error ? error.message : 'Failed to select terminal';
        });
      }
    },

  };
}
