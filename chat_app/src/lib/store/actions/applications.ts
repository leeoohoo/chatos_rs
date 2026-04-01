import type { Application } from '../../../types';
import type ApiClient from '../../api/client';
import type {
  ApplicationCreatePayload,
  ApplicationResponse,
  ApplicationUpdatePayload,
} from '../../api/client/types';
import type { AgentConfig, SystemContext } from '../../../types';
import type { ChatActions, ChatState, ChatStoreDraft } from '../types';
import { debugLog } from '@/lib/utils';

interface Deps {
  set: (updater: (state: ChatStoreDraft) => void) => void;
  get: () => ChatState & ChatActions;
  client: ApiClient;
  getUserIdParam: () => string;
}

export function createApplicationActions({ set, get, client, getUserIdParam }: Deps) {
  const toDate = (value?: string): Date => {
    if (!value) {
      return new Date();
    }

    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? new Date() : parsed;
  };

  const toFrontendApp = (apiApp: ApplicationResponse): Application => ({
    id: apiApp.id,
    name: apiApp.name,
    url: apiApp.url,
    iconUrl: apiApp.icon_url ?? apiApp.iconUrl ?? undefined,
    createdAt: toDate(apiApp.created_at),
    updatedAt: toDate(apiApp.updated_at ?? apiApp.created_at),
  });

  const removeAppAssociation = <T extends { app_ids?: string[] }>(item: T, appId: string): T => ({
    ...item,
    app_ids: Array.isArray(item.app_ids) ? item.app_ids.filter((id) => id !== appId) : [],
  });

  return {
    loadApplications: async () => {
      try {
        debugLog('[Store] loadApplications: start');
        const userId = getUserIdParam();
        const items = await client.getApplications(userId);
        const apps: Application[] = (items || []).map(toFrontendApp);
        set((state) => {
          state.applications = apps;
        });
        debugLog('[Store] loadApplications: loaded', apps.length, 'items');
      } catch (error) {
        console.error('Failed to load applications:', error);
        set((state) => {
          state.applications = [];
        });
      }
    },
    createApplication: async (name: string, url: string, iconUrl?: string) => {
      try {
        const payload: ApplicationCreatePayload = {
          name,
          url,
          icon_url: iconUrl ?? null,
          user_id: getUserIdParam(),
        };
        const created = await client.createApplication(payload);
        const app = toFrontendApp(created);
        set((state) => {
          state.applications = [app, ...state.applications];
        });
      } catch (error) {
        console.error('Failed to create application:', error);
        set((state) => {
          state.error = error instanceof Error ? error.message : 'Failed to create application';
        });
      }
    },
    updateApplication: async (id: string, updates: Partial<Application>) => {
      try {
        const payload: ApplicationUpdatePayload = {};
        if (updates.name !== undefined) payload.name = updates.name;
        if (updates.url !== undefined) payload.url = updates.url;
        if (updates.iconUrl !== undefined) payload.icon_url = updates.iconUrl ?? null;
        const updated = await client.updateApplication(id, payload);
        const nextApp = toFrontendApp(updated);
        set((state) => {
          state.applications = state.applications.map((a: Application) =>
            a.id === id ? nextApp : a
          );
        });
      } catch (error) {
        console.error('Failed to update application:', error);
        set((state) => {
          state.error = error instanceof Error ? error.message : 'Failed to update application';
        });
      }
    },
    deleteApplication: async (id: string) => {
      try {
        await client.deleteApplication(id);
        set((state) => {
          state.applications = state.applications.filter((a: Application) => a.id !== id);
          state.systemContexts = state.systemContexts.map((context: SystemContext) =>
            removeAppAssociation(context, id),
          );
          state.agents = state.agents.map((agent: AgentConfig) => removeAppAssociation(agent, id));
        });
      } catch (error) {
        console.error('Failed to delete application:', error);
        set((state) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete application';
        });
      }
    },
    setSelectedApplication: (appId: string | null) => {
      debugLog('[Store] setSelectedApplication:', appId);
      const oldId = get().selectedApplicationId;
      debugLog('[Store] setSelectedApplication - before:', { oldId, newId: appId });

      set((state) => {
        state.selectedApplicationId = appId;
      });

      const newId = get().selectedApplicationId;
      debugLog('[Store] setSelectedApplication - after:', { oldId, newId });
    },
    setSystemContextAppAssociation: (contextId: string, appIds: string[]) => {
      const normalizedAppIds = Array.isArray(appIds) ? appIds : [];
      const currentContext = get().systemContexts.find((context) => context.id === contextId);

      set((state) => {
        state.systemContexts = state.systemContexts.map((context) =>
          context.id === contextId
            ? { ...context, app_ids: normalizedAppIds }
            : context,
        );
      });

      if (!currentContext) {
        return;
      }

      void client.updateSystemContext(contextId, {
        name: currentContext.name,
        content: currentContext.content,
        app_ids: normalizedAppIds,
      }).catch((error) => {
        console.error('Failed to persist system context app association:', error);
      });
    },
    setAgentAppAssociation: (agentId: string, appIds: string[]) => {
      const normalizedAppIds = Array.isArray(appIds) ? appIds : [];
      set((state) => {
        state.agents = state.agents.map((agent) =>
          agent.id === agentId
            ? { ...agent, app_ids: normalizedAppIds }
            : agent,
        );
      });
    },
  };
}
