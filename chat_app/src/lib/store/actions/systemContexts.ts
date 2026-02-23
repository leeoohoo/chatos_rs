import type ApiClient from '../../api/client';

interface Deps {
  set: any;
  client: ApiClient;
  getUserIdParam: () => string;
}

export function createSystemContextActions({ set, client, getUserIdParam }: Deps) {
  return {
    loadSystemContexts: async () => {
      try {
        const contexts = await client.getSystemContexts(getUserIdParam());
        const activeContextResponse = await client.getActiveSystemContext(getUserIdParam());
        set((state: any) => {
          const updatedContexts = (contexts || []).map((ctx: any) => ({
            ...ctx,
            isActive: false,
          }));

          if (activeContextResponse && activeContextResponse.context) {
            const activeContext = activeContextResponse.context;
            const activeIndex = updatedContexts.findIndex(ctx => ctx.id === activeContext.id);
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
        set((state: any) => {
          state.systemContexts = [];
          state.activeSystemContext = null;
        });
      }
    },

    createSystemContext: async (name: string, content: string, appIds?: string[]) => {
      try {
        const context = await client.createSystemContext({
          name,
          content,
          user_id: getUserIdParam(),
          app_ids: Array.isArray(appIds) ? appIds : undefined,
        });
        set((state: any) => {
          state.systemContexts.push(context);
        });
        return context;
      } catch (error) {
        console.error('Failed to create system context:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to create system context';
        });
        return null;
      }
    },

    updateSystemContext: async (id: string, name: string, content: string, appIds?: string[]) => {
      try {
        const updatedContext = await client.updateSystemContext(id, { name, content, app_ids: Array.isArray(appIds) ? appIds : undefined });
        set((state: any) => {
          const index = state.systemContexts.findIndex((c: any) => c.id === id);
          if (index !== -1) {
            state.systemContexts[index] = updatedContext;
          }
        });
        return updatedContext;
      } catch (error) {
        console.error('Failed to update system context:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to update system context';
        });
        return null;
      }
    },

    deleteSystemContext: async (id: string) => {
      try {
        await client.deleteSystemContext(id);
        set((state: any) => {
          state.systemContexts = state.systemContexts.filter((c: any) => c.id !== id);
          if (state.activeSystemContext?.id === id) {
            state.activeSystemContext = null;
          }
        });
      } catch (error) {
        console.error('Failed to delete system context:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete system context';
        });
      }
    },

    activateSystemContext: async (id: string) => {
      try {
        await client.activateSystemContext(id, getUserIdParam());
        set((state: any) => {
          const context = state.systemContexts.find((c: any) => c.id === id);
          if (context) {
            state.systemContexts.forEach((ctx: any) => {
              ctx.isActive = ctx.id === id;
            });
            state.activeSystemContext = { ...context, isActive: true };
          }
        });
      } catch (error) {
        console.error('Failed to activate system context:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to activate system context';
        });
      }
    },

    generateSystemContextDraft: async (payload: {
      scene: string;
      style?: string;
      language?: string;
      output_format?: string;
      constraints?: string[];
      forbidden?: string[];
      candidate_count?: number;
      ai_model_config?: any;
    }) => {
      try {
        return await client.generateSystemContextDraft({
          user_id: getUserIdParam(),
          ...payload,
        });
      } catch (error) {
        console.error('Failed to generate system context draft:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to generate system context draft';
        });
        return null;
      }
    },

    optimizeSystemContextDraft: async (payload: {
      content: string;
      goal?: string;
      keep_intent?: boolean;
      ai_model_config?: any;
    }) => {
      try {
        return await client.optimizeSystemContextDraft({
          user_id: getUserIdParam(),
          ...payload,
        });
      } catch (error) {
        console.error('Failed to optimize system context draft:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to optimize system context draft';
        });
        return null;
      }
    },

    evaluateSystemContextDraft: async (payload: { content: string }) => {
      try {
        return await client.evaluateSystemContextDraft(payload);
      } catch (error) {
        console.error('Failed to evaluate system context draft:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to evaluate system context draft';
        });
        return null;
      }
    },
  };
}
