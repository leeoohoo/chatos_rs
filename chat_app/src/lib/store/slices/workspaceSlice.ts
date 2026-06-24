import type { Project } from '../../../types';

export type ActivePanel = 'chat' | 'project' | 'terminal' | 'remote_terminal' | 'remote_sftp';

export interface WorkspaceSliceState {
  projects: Project[];
  currentProjectId: string | null;
  currentProject: Project | null;
  activePanel: ActivePanel;
}

export const workspaceInitialState: WorkspaceSliceState = {
  projects: [],
  currentProjectId: null,
  currentProject: null,
  activePanel: 'chat',
};

export interface WorkspaceSliceActions {
  loadProjects: (options?: { force?: boolean }) => Promise<Project[]>;
  createProject: (name: string, rootPath: string, description?: string, gitUrl?: string) => Promise<Project>;
  updateProject: (projectId: string, updates: Partial<Project>) => Promise<Project | null>;
  deleteProject: (projectId: string) => Promise<void>;
  selectProject: (projectId: string) => Promise<void>;
  markProjectsStale: (options?: { userId?: string | null; projectId?: string | null }) => void;
  removeProjectLocally: (projectId: string) => void;
  applyRealtimeProjectSnapshot: (project: Project | unknown) => Project | null;
  refreshProjectById: (projectId: string) => Promise<Project | null>;
  setActivePanel: (panel: ActivePanel) => void;
}
