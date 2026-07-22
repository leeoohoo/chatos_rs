// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Project } from '../../../types';
import type ApiClient from '../../api/client';
import { ApiRequestError } from '../../api/client/shared';
import { localRuntimeBridgeAvailable } from '../../api/localRuntime';
import { resolveProjectExecutionPlane } from '../../domain/projectExecution';
import { isTransientServiceAppError } from '../../domain/userVisibleError';
import { normalizeProject } from '../helpers/projects';
import type { ChatStoreDraft, ChatStoreGet, ChatStoreSet } from '../types';

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
  getUserIdParam: () => string;
}

interface LoadProjectsOptions {
  force?: boolean;
  throwOnError?: boolean;
}

interface ProjectsListCacheEntry {
  projects: Project[];
  stale: boolean;
}

interface ProjectsDetailCacheEntry {
  project: Project;
  stale: boolean;
}

interface ProjectsClientCacheState {
  listCache: Map<string, ProjectsListCacheEntry>;
  listInflight: Map<string, Promise<Project[]>>;
  detailCache: Map<string, ProjectsDetailCacheEntry>;
  detailInflight: Map<string, Promise<Project>>;
}

const projectsClientCaches = new WeakMap<ApiClient, ProjectsClientCacheState>();

const normalizeUserId = (userId: string): string => String(userId || '').trim();

const normalizeProjectId = (projectId: string): string => String(projectId || '').trim();

const projectIsVisibleOnCurrentSurface = (project: Project): boolean => (
  localRuntimeBridgeAvailable()
  || resolveProjectExecutionPlane(project) === 'cloud'
);

const visibleProjectsOnCurrentSurface = (projects: Project[]): Project[] => (
  projects.filter(projectIsVisibleOnCurrentSurface)
);

const getOrCreateClientCacheState = (apiClient: ApiClient): ProjectsClientCacheState => {
  const existing = projectsClientCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: ProjectsClientCacheState = {
    listCache: new Map(),
    listInflight: new Map(),
    detailCache: new Map(),
    detailInflight: new Map(),
  };
  projectsClientCaches.set(apiClient, next);
  return next;
};

const upsertProject = (projects: Project[], project: Project): Project[] => {
  const index = projects.findIndex((item) => item.id === project.id);
  if (index === -1) {
    return [project, ...projects];
  }
  const next = [...projects];
  next[index] = project;
  return next;
};

const markProjectCachesStale = (
  apiClient: ApiClient,
  options?: { userId?: string | null; projectId?: string | null },
) => {
  const cacheState = getOrCreateClientCacheState(apiClient);
  const normalizedUserId = normalizeUserId(String(options?.userId || ''));
  const normalizedProjectId = normalizeProjectId(String(options?.projectId || ''));

  if (normalizedUserId) {
    const cached = cacheState.listCache.get(normalizedUserId);
    if (cached) {
      cacheState.listCache.set(normalizedUserId, {
        ...cached,
        stale: true,
      });
    }
  } else {
    cacheState.listCache.forEach((entry, key) => {
      cacheState.listCache.set(key, {
        ...entry,
        stale: true,
      });
    });
  }

  if (normalizedProjectId) {
    const cached = cacheState.detailCache.get(normalizedProjectId);
    if (cached) {
      cacheState.detailCache.set(normalizedProjectId, {
        ...cached,
        stale: true,
      });
    }
  }
};

export function createProjectActions({ set, get, client, getUserIdParam }: Deps) {
  const syncProjectsIntoState = (formatted: Project[], uid: string) => {
    set((state: ChatStoreDraft) => {
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
        } else {
          state.currentProjectId = null;
          state.currentProject = null;
          if (state.activePanel === 'project') {
            state.activePanel = 'chat';
          }
        }
      }
    });
  };

  const syncProjectDetailCache = (project: Project) => {
    const normalizedProjectId = normalizeProjectId(project.id);
    if (!normalizedProjectId) {
      return;
    }
    client.registerProjectExecution(project);
    getOrCreateClientCacheState(client).detailCache.set(normalizedProjectId, {
      project,
      stale: false,
    });
  };

  const syncProjectListCaches = (updater: (projects: Project[]) => Project[]) => {
    const cacheState = getOrCreateClientCacheState(client);
    cacheState.listCache.forEach((entry, key) => {
      cacheState.listCache.set(key, {
        projects: updater(entry.projects),
        stale: false,
      });
    });
  };

  const syncLoadedProjects = (userId: string, projects: Project[]) => {
    const cacheState = getOrCreateClientCacheState(client);
    cacheState.listCache.set(normalizeUserId(userId), {
      projects,
      stale: false,
    });
    projects.forEach((project) => {
      syncProjectDetailCache(project);
    });
  };

  const upsertProjectCaches = (project: Project) => {
    syncProjectDetailCache(project);
    syncProjectListCaches((projects) => upsertProject(projects, project));
  };

  const removeProjectCaches = (projectId: string) => {
    const normalizedProjectId = normalizeProjectId(projectId);
    if (!normalizedProjectId) {
      return;
    }
    const cacheState = getOrCreateClientCacheState(client);
    cacheState.detailCache.delete(normalizedProjectId);
    cacheState.detailInflight.delete(normalizedProjectId);
    syncProjectListCaches((projects) => projects.filter((project) => project.id !== normalizedProjectId));
  };

  const loadProjectDetail = async (projectId: string): Promise<Project> => {
    const normalizedProjectId = normalizeProjectId(projectId);
    if (!normalizedProjectId) {
      throw new Error('project id is required');
    }
    const cacheState = getOrCreateClientCacheState(client);
    const cached = cacheState.detailCache.get(normalizedProjectId);
    if (cached && !cached.stale) {
      return cached.project;
    }
    let inflight = cacheState.detailInflight.get(normalizedProjectId);
    if (!inflight) {
      inflight = client.getProject(normalizedProjectId)
        .then((payload) => normalizeProject(payload))
        .then((project) => {
          if (!projectIsVisibleOnCurrentSurface(project)) {
            throw new ApiRequestError('本地项目只能在 Chat OS 桌面客户端中打开', {
              status: 404,
              code: 'project_not_found',
            });
          }
          syncProjectDetailCache(project);
          syncProjectListCaches((projects) => upsertProject(projects, project));
          return project;
        })
        .finally(() => {
          cacheState.detailInflight.delete(normalizedProjectId);
        });
      cacheState.detailInflight.set(normalizedProjectId, inflight);
    }
    return inflight;
  };

  return {
    applyRealtimeProjectSnapshot: (projectPayload: Project | unknown) => {
      const project = normalizeProject(projectPayload);
      const normalizedProjectId = normalizeProjectId(project?.id || '');
      if (!normalizedProjectId) {
        return null;
      }
      if (!projectIsVisibleOnCurrentSurface(project)) {
        removeProjectCaches(normalizedProjectId);
        set((state: ChatStoreDraft) => {
          state.projects = state.projects.filter((item) => item.id !== normalizedProjectId);
          if (state.currentProjectId === normalizedProjectId) {
            state.currentProjectId = null;
            state.currentProject = null;
          }
        });
        return null;
      }
      client.registerProjectExecution(project);
      upsertProjectCaches(project);
      set((state: ChatStoreDraft) => {
        state.projects = upsertProject(state.projects, project);
        if (state.currentProjectId === normalizedProjectId) {
          state.currentProject = project;
        }
      });
      return project;
    },

    loadProjects: async (options?: LoadProjectsOptions) => {
      try {
        const uid = getUserIdParam();
        const cacheKey = normalizeUserId(uid);
        const cacheState = getOrCreateClientCacheState(client);
        const cached = cacheState.listCache.get(cacheKey);
        if (!options?.force && cached && !cached.stale) {
          const formatted = cached.projects;
          syncProjectsIntoState(formatted, uid);
          return formatted;
        }

        let inflight = options?.force ? undefined : cacheState.listInflight.get(cacheKey);
        if (!inflight) {
          const request = client.listProjects(uid)
            .then((list) => {
              const formatted = visibleProjectsOnCurrentSurface(
                Array.isArray(list) ? list.map(normalizeProject) : [],
              );
              return formatted;
            });
          const trackedRequest: Promise<Project[]> = request.then((formatted) => {
            if (cacheState.listInflight.get(cacheKey) === trackedRequest) {
              syncLoadedProjects(uid, formatted);
              syncProjectsIntoState(formatted, uid);
              set((state: ChatStoreDraft) => {
                if (isTransientServiceAppError(state.error)) {
                  state.error = null;
                }
              });
              cacheState.listInflight.delete(cacheKey);
            }
            return formatted;
          }, (error) => {
            if (cacheState.listInflight.get(cacheKey) === trackedRequest) {
              cacheState.listInflight.delete(cacheKey);
            }
            throw error;
          });
          inflight = trackedRequest;
          cacheState.listInflight.set(cacheKey, inflight);
        }

        const formatted = await inflight;
        return formatted;
      } catch (error) {
        console.error('Failed to load projects:', error);
        if (options?.throwOnError) {
          throw error;
        }
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to load projects';
        });
        return get().projects || [];
      }
    },

    createProject: async (name: string, rootPath: string, description?: string, gitUrl?: string) => {
      const uid = getUserIdParam();
      const payload = {
        name,
        root_path: rootPath,
        git_url: gitUrl?.trim() || undefined,
        description: description?.trim() || undefined,
        user_id: uid,
      };
      const created = await client.createProject(payload);
      const project = normalizeProject(created);
      upsertProjectCaches(project);
      set((state: ChatStoreDraft) => {
        state.projects = upsertProject(state.projects, project);
        state.currentProjectId = project.id;
        state.currentProject = project;
        state.activePanel = 'project';
      });
      localStorage.setItem(`lastProjectId_${uid}`, project.id);
      return project;
    },

    createCloudProject: async (input: {
      name: string;
      gitUrl?: string;
      zipFile?: File | null;
      description?: string;
    }) => {
      const uid = getUserIdParam();
      const form = new FormData();
      form.set('name', input.name.trim());
      const gitUrl = input.gitUrl?.trim();
      if (gitUrl) {
        form.set('git_url', gitUrl);
      }
      const description = input.description?.trim();
      if (description) {
        form.set('description', description);
      }
      if (input.zipFile) {
        form.set('zip', input.zipFile, input.zipFile.name || 'project.zip');
      }
      const created = await client.createCloudProject(form);
      const project = normalizeProject(created);
      upsertProjectCaches(project);
      set((state: ChatStoreDraft) => {
        state.projects = upsertProject(state.projects, project);
        state.currentProjectId = project.id;
        state.currentProject = project;
        state.activePanel = 'project';
      });
      localStorage.setItem(`lastProjectId_${uid}`, project.id);
      return project;
    },

    updateProject: async (projectId: string, updates: Partial<Project>) => {
      try {
        const payload: { name?: string; root_path?: string; git_url?: string; description?: string } = {};
        if (updates.name !== undefined) payload.name = updates.name;
        if (updates.rootPath !== undefined) payload.root_path = updates.rootPath;
        if (updates.gitUrl !== undefined) payload.git_url = updates.gitUrl ?? '';
        if (updates.description !== undefined) payload.description = updates.description || undefined;
        const updated = await client.updateProject(projectId, payload);
        const project = normalizeProject(updated);
        upsertProjectCaches(project);
        set((state: ChatStoreDraft) => {
          state.projects = upsertProject(state.projects, project);
          if (state.currentProjectId === projectId) {
            state.currentProject = project;
          }
        });
        return project;
      } catch (error) {
        console.error('Failed to update project:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to update project';
        });
        return null;
      }
    },

    deleteProject: async (projectId: string) => {
      try {
        await client.deleteProject(projectId);
        removeProjectCaches(projectId);
        set((state: ChatStoreDraft) => {
          state.projects = state.projects.filter((project) => project.id !== projectId);
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
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete project';
        });
      }
    },

    selectProject: async (projectId: string) => {
      try {
        const normalizedProjectId = normalizeProjectId(projectId);
        let project = get().projects.find((item: Project) => item.id === normalizedProjectId) || null;
        if (!project) {
          project = await loadProjectDetail(normalizedProjectId);
        }
        await client.prepareProjectRuntime(project);
        const uid = getUserIdParam();
        set((state: ChatStoreDraft) => {
          state.projects = upsertProject(state.projects, project);
          state.currentProjectId = normalizedProjectId;
          state.currentProject = project;
          state.activePanel = 'project';
        });
        localStorage.setItem(`lastProjectId_${uid}`, normalizedProjectId);
      } catch (error) {
        console.error('Failed to select project:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to select project';
        });
      }
    },

    setActivePanel: (panel: 'chat' | 'project' | 'terminal' | 'remote_terminal' | 'remote_sftp') => {
      set((state: ChatStoreDraft) => {
        state.activePanel = panel;
      });
    },

    markProjectsStale: (options?: { userId?: string | null; projectId?: string | null }) => {
      markProjectCachesStale(client, options);
    },

    removeProjectLocally: (projectId: string) => {
      const normalizedProjectId = normalizeProjectId(projectId);
      if (!normalizedProjectId) {
        return;
      }
      removeProjectCaches(normalizedProjectId);
      set((state: ChatStoreDraft) => {
        state.projects = state.projects.filter((project) => project.id !== normalizedProjectId);
        if (state.currentProjectId === normalizedProjectId) {
          state.currentProjectId = null;
          state.currentProject = null;
          if (state.activePanel === 'project') {
            state.activePanel = 'chat';
          }
        }
      });
    },

    refreshProjectById: async (projectId: string) => {
      try {
        const normalizedProjectId = normalizeProjectId(projectId);
        if (!normalizedProjectId) {
          return null;
        }
        const project = await loadProjectDetail(normalizedProjectId);
        set((state: ChatStoreDraft) => {
          state.projects = upsertProject(state.projects, project);
          if (state.currentProjectId === normalizedProjectId) {
            state.currentProject = project;
          }
        });
        return project;
      } catch (error) {
        if (error instanceof ApiRequestError && error.status === 404) {
          removeProjectCaches(projectId);
          set((state: ChatStoreDraft) => {
            state.projects = state.projects.filter((project) => project.id !== projectId);
            if (state.currentProjectId === projectId) {
              state.currentProjectId = null;
              state.currentProject = null;
              if (state.activePanel === 'project') {
                state.activePanel = 'chat';
              }
            }
          });
          return null;
        }
        console.error('Failed to refresh project detail:', error);
        return null;
      }
    },
  };
}
