// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type ApiClient from '../../api/client';
import type {
  ActiveSystemContextResponse,
  SystemContextDraftEvaluatePayload,
  SystemContextDraftEvaluateResponse,
  SystemContextDraftGeneratePayload,
  SystemContextDraftGenerateResponse,
  SystemContextDraftOptimizePayload,
  SystemContextDraftOptimizeResponse,
  SystemContextResponse,
} from '../../api/client/types';
import type { ChatStoreDraft, ChatStoreSet } from '../types';
import { normalizeSystemContext } from '../../domain/configs';

interface Deps {
  set: ChatStoreSet;
  client: ApiClient;
  getUserIdParam: () => string;
}

export function createSystemContextActions({ set, client, getUserIdParam }: Deps) {
  return {
    loadSystemContexts: async () => {
      try {
        const contexts = await client.getSystemContexts(getUserIdParam());
        const activeContextResponse = await client.getActiveSystemContext(getUserIdParam());
        set((state: ChatStoreDraft) => {
          const updatedContexts = (contexts || []).map((ctx) => ({
            ...normalizeSystemContext(ctx),
            isActive: false,
          }));

          const activeContext = (activeContextResponse as ActiveSystemContextResponse | null)?.context;
          if (activeContext) {
            const activeIndex = updatedContexts.findIndex((ctx) => ctx.id === activeContext.id);
            if (activeIndex !== -1) {
              updatedContexts[activeIndex].isActive = true;
              state.activeSystemContext = { ...updatedContexts[activeIndex] };
            } else {
              state.activeSystemContext = null;
            }
          } else {
            state.activeSystemContext = null;
          }

          state.systemContexts = updatedContexts;
        });
      } catch (error) {
        console.error('Failed to load system contexts:', error);
        set((state: ChatStoreDraft) => {
          state.systemContexts = [];
          state.activeSystemContext = null;
        });
      }
    },

    createSystemContext: async (
      name: string,
      content: string,
      appIds?: string[],
    ): Promise<SystemContextResponse | null> => {
      try {
        const context = await client.createSystemContext({
          name,
          content,
          user_id: getUserIdParam(),
          app_ids: Array.isArray(appIds) ? appIds : undefined,
        });
        const normalized = normalizeSystemContext(context);
        set((state: ChatStoreDraft) => {
          state.systemContexts.push(normalized);
        });
        return context;
      } catch (error) {
        console.error('Failed to create system context:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to create system context';
        });
        return null;
      }
    },

    updateSystemContext: async (
      id: string,
      name: string,
      content: string,
      appIds?: string[],
    ): Promise<SystemContextResponse | null> => {
      try {
        const updatedContext = await client.updateSystemContext(id, { name, content, app_ids: Array.isArray(appIds) ? appIds : undefined });
        const normalized = normalizeSystemContext(updatedContext);
        set((state: ChatStoreDraft) => {
          const index = state.systemContexts.findIndex((context) => context.id === id);
          if (index !== -1) {
            state.systemContexts[index] = normalized;
          }
        });
        return updatedContext;
      } catch (error) {
        console.error('Failed to update system context:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to update system context';
        });
        return null;
      }
    },

    deleteSystemContext: async (id: string) => {
      try {
        await client.deleteSystemContext(id);
        set((state: ChatStoreDraft) => {
          state.systemContexts = state.systemContexts.filter((context) => context.id !== id);
          if (state.activeSystemContext?.id === id) {
            state.activeSystemContext = null;
          }
        });
      } catch (error) {
        console.error('Failed to delete system context:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete system context';
        });
      }
    },

    activateSystemContext: async (id: string) => {
      try {
        await client.activateSystemContext(id, getUserIdParam());
        set((state: ChatStoreDraft) => {
          const context = state.systemContexts.find((item) => item.id === id);
          if (context) {
            state.systemContexts.forEach((ctx) => {
              ctx.isActive = ctx.id === id;
            });
            state.activeSystemContext = { ...context, isActive: true };
          }
        });
      } catch (error) {
        console.error('Failed to activate system context:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to activate system context';
        });
      }
    },

    generateSystemContextDraft: async (
      payload: Omit<SystemContextDraftGeneratePayload, 'user_id'>,
    ): Promise<SystemContextDraftGenerateResponse> => {
      try {
        return await client.generateSystemContextDraft({
          user_id: getUserIdParam(),
          ...payload,
        });
      } catch (error) {
        console.error('Failed to generate system context draft:', error);
        const message = error instanceof Error ? error.message : 'Failed to generate system context draft';
        set((state: ChatStoreDraft) => {
          state.error = message;
        });
        throw (error instanceof Error ? error : new Error(message));
      }
    },

    optimizeSystemContextDraft: async (
      payload: Omit<SystemContextDraftOptimizePayload, 'user_id'>,
    ): Promise<SystemContextDraftOptimizeResponse> => {
      try {
        return await client.optimizeSystemContextDraft({
          user_id: getUserIdParam(),
          ...payload,
        });
      } catch (error) {
        console.error('Failed to optimize system context draft:', error);
        const message = error instanceof Error ? error.message : 'Failed to optimize system context draft';
        set((state: ChatStoreDraft) => {
          state.error = message;
        });
        throw (error instanceof Error ? error : new Error(message));
      }
    },

    evaluateSystemContextDraft: async (
      payload: SystemContextDraftEvaluatePayload,
    ): Promise<SystemContextDraftEvaluateResponse> => {
      try {
        return await client.evaluateSystemContextDraft(payload);
      } catch (error) {
        console.error('Failed to evaluate system context draft:', error);
        const message = error instanceof Error ? error.message : 'Failed to evaluate system context draft';
        set((state: ChatStoreDraft) => {
          state.error = message;
        });
        throw (error instanceof Error ? error : new Error(message));
      }
    },
  };
}
