import { useMemo } from 'react';

import type { Project } from '../../types';
import { useProjectExplorerWorkspaceView } from './useProjectExplorerWorkspaceView';
import type { ProjectExplorerWorkspaceShellParams } from './workspaceViewTypes';

const createPlaceholderProject = (): Project => ({
  id: '__placeholder__',
  name: '',
  rootPath: '',
  createdAt: new Date(0),
  updatedAt: new Date(0),
});

export const useProjectExplorerWorkspaceShell = ({
  project,
  ...params
}: ProjectExplorerWorkspaceShellParams) => {
  const effectiveProject = useMemo(
    () => project ?? createPlaceholderProject(),
    [project],
  );

  return useProjectExplorerWorkspaceView({
    ...params,
    project: effectiveProject,
  });
};
