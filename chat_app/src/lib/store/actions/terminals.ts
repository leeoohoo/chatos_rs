import type { Terminal } from '../../../types';
import type ApiClient from '../../api/client';
import { ApiRequestError } from '../../api/client/shared';
import { normalizeTerminal } from '../../domain/terminals';
import type { ChatStoreDraft, ChatStoreGet, ChatStoreSet } from '../types';
import {
  loadTerminalDetailSnapshot,
  loadTerminalsSnapshot,
  markTerminalCachesStale,
  removeTerminal,
  removeTerminalCaches,
  upsertTerminal,
  upsertTerminalCaches,
} from './terminalsCache';

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
  getUserIdParam: () => string;
}

interface LoadTerminalsOptions {
  force?: boolean;
}

const syncCurrentTerminalSelection = (
  state: ChatStoreDraft,
  uid: string,
  terminals: Terminal[],
) => {
  state.terminals = terminals;
  if (!state.currentTerminalId) {
    const lastId = localStorage.getItem(`lastTerminalId_${uid}`);
    if (lastId) {
      const matched = terminals.find((item) => item.id === lastId);
      if (matched) {
        state.currentTerminalId = matched.id;
        state.currentTerminal = matched;
      }
    }
    return;
  }

  const matched = terminals.find((item) => item.id === state.currentTerminalId);
  if (matched) {
    state.currentTerminal = matched;
    return;
  }

  state.currentTerminalId = null;
  state.currentTerminal = null;
  if (state.activePanel === 'terminal') {
    state.activePanel = 'chat';
  }
};

export function createTerminalActions({ set, get, client, getUserIdParam }: Deps) {
  return {
    applyRealtimeTerminalSnapshot: (terminalPayload: Terminal | unknown) => {
      const terminal = normalizeTerminal(terminalPayload);
      const normalizedTerminalId = String(terminal?.id || '').trim();
      if (!normalizedTerminalId) {
        return null;
      }
      upsertTerminalCaches(client, terminal);
      const uid = getUserIdParam();
      set((state: ChatStoreDraft) => {
        state.terminals = upsertTerminal(state.terminals, terminal);
        if (state.currentTerminalId === normalizedTerminalId) {
          state.currentTerminal = terminal;
        } else {
          syncCurrentTerminalSelection(state, uid, state.terminals);
        }
      });
      return terminal;
    },

    loadTerminals: async (options?: LoadTerminalsOptions) => {
      try {
        const uid = getUserIdParam();
        const formatted = await loadTerminalsSnapshot(client, uid, options);
        set((state: ChatStoreDraft) => {
          syncCurrentTerminalSelection(state, uid, formatted);
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
      const normalizedTerminal = normalizeTerminal(created);
      upsertTerminalCaches(client, normalizedTerminal);
      set((state: ChatStoreDraft) => {
        state.terminals = upsertTerminal(state.terminals, normalizedTerminal);
        state.currentTerminalId = normalizedTerminal.id;
        state.currentTerminal = normalizedTerminal;
        state.activePanel = 'terminal';
      });
      localStorage.setItem(`lastTerminalId_${uid}`, normalizedTerminal.id);
      return normalizedTerminal;
    },

    deleteTerminal: async (terminalId: string) => {
      try {
        await client.deleteTerminal(terminalId);
        removeTerminalCaches(client, terminalId);
        set((state: ChatStoreDraft) => {
          state.terminals = removeTerminal(state.terminals, terminalId);
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
          terminal = await loadTerminalDetailSnapshot(client, terminalId);
        }
        if (!terminal) {
          throw new ApiRequestError('终端不存在', { status: 404 });
        }
        const uid = getUserIdParam();
        set((state: ChatStoreDraft) => {
          state.terminals = upsertTerminal(state.terminals, terminal);
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

    markTerminalsStale: (options?: { userId?: string | null; terminalId?: string | null }) => {
      markTerminalCachesStale(client, options);
    },

    removeTerminalLocally: (terminalId: string) => {
      removeTerminalCaches(client, terminalId);
      set((state: ChatStoreDraft) => {
        state.terminals = removeTerminal(state.terminals, terminalId);
        if (state.currentTerminalId === terminalId) {
          state.currentTerminalId = null;
          state.currentTerminal = null;
          if (state.activePanel === 'terminal') {
            state.activePanel = 'chat';
          }
        }
      });
    },

    refreshTerminalById: async (terminalId: string) => {
      try {
        const normalized = String(terminalId || '').trim();
        if (!normalized) {
          return null;
        }
        const terminal = await loadTerminalDetailSnapshot(client, normalized, { force: true });
        if (!terminal) {
          set((state: ChatStoreDraft) => {
            state.terminals = removeTerminal(state.terminals, normalized);
            if (state.currentTerminalId === normalized) {
              state.currentTerminalId = null;
              state.currentTerminal = null;
              if (state.activePanel === 'terminal') {
                state.activePanel = 'chat';
              }
            }
          });
          return null;
        }
        const uid = getUserIdParam();
        set((state: ChatStoreDraft) => {
          state.terminals = upsertTerminal(state.terminals, terminal);
          if (state.currentTerminalId === normalized) {
            state.currentTerminal = terminal;
          } else {
            syncCurrentTerminalSelection(state, uid, state.terminals);
          }
        });
        return terminal;
      } catch (error) {
        console.error('Failed to refresh terminal detail:', error);
        return null;
      }
    },

  };
}
