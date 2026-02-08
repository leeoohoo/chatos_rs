import type { Application } from '../../../types';
import { debugLog } from '@/lib/utils';

interface Deps {
  set: any;
  get: any;
  client: any;
  getUserIdParam: () => string;
}

export function createApplicationActions({ set, get, client, getUserIdParam }: Deps) {
  const toFrontendApp = (apiApp: any): Application => ({
    id: apiApp.id,
    name: apiApp.name,
    url: apiApp.url,
    iconUrl: apiApp.icon_url ?? undefined,
    createdAt: new Date(apiApp.created_at),
    updatedAt: new Date(apiApp.updated_at ?? apiApp.created_at),
  });

  return {
    // 应用管理
    loadApplications: async () => {
      try {
        debugLog('[Store] loadApplications: start');
        const userId = getUserIdParam();
        const items = await client.getApplications(userId);
        const apps: Application[] = (items || []).map(toFrontendApp);
        set((state: any) => {
          state.applications = apps;
        });
        debugLog('[Store] loadApplications: loaded', apps.length, 'items');
      } catch (error) {
        console.error('Failed to load applications:', error);
        set((state: any) => {
          state.applications = [];
        });
      }
    },
    createApplication: async (name: string, url: string, iconUrl?: string) => {
      try {
        const created = await client.createApplication({
          name,
          url,
          icon_url: iconUrl ?? null,
          user_id: getUserIdParam(),
        });
        const app = toFrontendApp(created);
        set((state: any) => {
          state.applications = [app, ...state.applications];
        });
      } catch (error) {
        console.error('Failed to create application:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to create application';
        });
      }
    },
    updateApplication: async (id: string, updates: Partial<Application>) => {
      try {
        const payload: any = {};
        if (updates.name !== undefined) payload.name = updates.name;
        if (updates.url !== undefined) payload.url = updates.url;
        if (updates.iconUrl !== undefined) payload.icon_url = updates.iconUrl ?? null;
        const updated = await client.updateApplication(id, payload);
        const nextApp = toFrontendApp(updated);
        set((state: any) => {
          state.applications = state.applications.map((a: Application) =>
            a.id === id ? nextApp : a
          );
        });
      } catch (error) {
        console.error('Failed to update application:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to update application';
        });
      }
    },
    deleteApplication: async (id: string) => {
      try {
        await client.deleteApplication(id);
        set((state: any) => {
          state.applications = state.applications.filter((a: Application) => a.id !== id);
          // 其他类型（SystemContext/Agent）暂保持现状，待后端支持后统一切换
          state.systemContexts = state.systemContexts.map((c: any) => ({
            ...c,
            app_ids: Array.isArray(c.app_ids) ? c.app_ids.filter((aid: string) => aid !== id) : []
          }));
          state.agents = state.agents.map((a: any) => ({
            ...a,
            app_ids: Array.isArray(a.app_ids) ? a.app_ids.filter((aid: string) => aid !== id) : []
          }));
        });
      } catch (error) {
        console.error('Failed to delete application:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete application';
        });
      }
    },
    setSelectedApplication: (appId: string | null) => {
      debugLog('[Store] setSelectedApplication:', appId);
      const oldId = get().selectedApplicationId;
      debugLog('[Store] setSelectedApplication - before:', { oldId, newId: appId });

      // 强制创建新的状态对象，确保引用变化
      set((state: any) => ({ ...state, selectedApplicationId: appId }));

      const newId = get().selectedApplicationId;
      debugLog('[Store] setSelectedApplication - after:', { oldId, newId });
    },
    setSystemContextAppAssociation: (contextId: string, appIds: string[]) => {
      set((state: any) => {
        state.systemContexts = state.systemContexts.map((c: any) => (c.id === contextId ? { ...c, app_ids: Array.isArray(appIds) ? appIds : [] } : c));
      });
      // 后端持久化 app_ids（只更新关联，避免覆盖名称与内容）
      try {
        (async () => {
          await client.updateSystemContext(contextId, { name: undefined as any, content: undefined as any, app_ids: Array.isArray(appIds) ? appIds : [] });
        })();
      } catch {}
    },
    setAgentAppAssociation: (agentId: string, appIds: string[]) => {
      set((state: any) => {
        state.agents = state.agents.map((a: any) => (a.id === agentId ? { ...a, app_ids: Array.isArray(appIds) ? appIds : [] } : a));
      });
      // 后端持久化 app_ids 到 Agent
      try {
        (async () => {
          await client.updateAgent(agentId, { app_ids: Array.isArray(appIds) ? appIds : [] });
        })();
      } catch {}
    },
  };
}
