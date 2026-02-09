import type { Project } from '../../../types';
import type ApiClient from '../../api/client';
import { normalizeProject } from '../helpers/projects';

interface Deps {
  set: any;
  get: any;
  client: ApiClient;
  getUserIdParam: () => string;
}

export function createProjectActions({ set, get, client, getUserIdParam }: Deps) {
  return {
    loadProjects: async () => {
      try {
        const uid = getUserIdParam();
        const list = await client.listProjects(uid);
        const formatted = Array.isArray(list) ? list.map(normalizeProject) : [];
        set((state: any) => {
          state.projects = formatted;
          if (!state.currentProjectId) {
            const lastId = localStorage.getItem(`lastProjectId_${uid}`);
            if (lastId) {
              const matched = formatted.find(p => p.id === lastId);
              if (matched) {
                state.currentProjectId = matched.id;
                state.currentProject = matched;
              }
            }
          } else {
            const matched = formatted.find(p => p.id === state.currentProjectId);
            if (matched) {
              state.currentProject = matched;
            }
          }
        });
        return formatted;
      } catch (error) {
        console.error('Failed to load projects:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to load projects';
        });
        return [];
      }
    },

    createProject: async (name: string, rootPath: string, description?: string) => {
      const uid = getUserIdParam();
      const payload = {
        name,
        root_path: rootPath,
        description: description?.trim() || undefined,
        user_id: uid,
      };
      const created = await client.createProject(payload);
      const project = normalizeProject(created);
      set((state: any) => {
        state.projects.unshift(project);
        state.currentProjectId = project.id;
        state.currentProject = project;
        state.activePanel = 'project';
      });
      localStorage.setItem(`lastProjectId_${uid}`, project.id);
      return project;
    },

    updateProject: async (projectId: string, updates: Partial<Project>) => {
      try {
        const payload: { name?: string; root_path?: string; description?: string } = {};
        if (updates.name !== undefined) payload.name = updates.name;
        if (updates.rootPath !== undefined) payload.root_path = updates.rootPath;
        if (updates.description !== undefined) payload.description = updates.description || undefined;
        const updated = await client.updateProject(projectId, payload);
        const project = normalizeProject(updated);
        set((state: any) => {
          const index = state.projects.findIndex((p: any) => p.id === projectId);
          if (index !== -1) {
            state.projects[index] = project;
          }
          if (state.currentProjectId === projectId) {
            state.currentProject = project;
          }
        });
        return project;
      } catch (error) {
        console.error('Failed to update project:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to update project';
        });
        return null;
      }
    },

    deleteProject: async (projectId: string) => {
      try {
        await client.deleteProject(projectId);
        set((state: any) => {
          state.projects = state.projects.filter((p: any) => p.id !== projectId);
          if (state.currentProjectId === projectId) {
            state.currentProjectId = null;
            state.currentProject = null;
            if (state.activePanel === 'project') {
              state.activePanel = 'chat';
            }
          }
        });
      } catch (error) {
        console.error('Failed to delete project:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete project';
        });
      }
    },

    selectProject: async (projectId: string) => {
      try {
        let project = get().projects.find((p: any) => p.id === projectId) || null;
        if (!project) {
          const fetched = await client.getProject(projectId);
          project = normalizeProject(fetched);
        }
        const uid = getUserIdParam();
        set((state: any) => {
          state.currentProjectId = projectId;
          state.currentProject = project;
          state.activePanel = 'project';
        });
        localStorage.setItem(`lastProjectId_${uid}`, projectId);
      } catch (error) {
        console.error('Failed to select project:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to select project';
        });
      }
    },

    setActivePanel: (panel: 'chat' | 'project' | 'terminal') => {
      set((state: any) => {
        state.activePanel = panel;
      });
    },
  };
}
