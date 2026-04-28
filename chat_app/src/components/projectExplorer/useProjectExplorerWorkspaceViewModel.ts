import { useProjectExplorerWorkspaceShell } from './useProjectExplorerWorkspaceShell';
import type { ProjectExplorerWorkspaceShellParams } from './workspaceViewTypes';

export const useProjectExplorerWorkspaceViewModel = (
  params: ProjectExplorerWorkspaceShellParams,
) => useProjectExplorerWorkspaceShell(params);
