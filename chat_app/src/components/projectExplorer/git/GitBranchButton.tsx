import React from 'react';

import {
  GitBranchDialogMounts,
  GitBranchDropdown,
  GitBranchTrigger,
} from './GitBranchButtonViews';
import type { ProjectGitApiClient } from './projectGitTypes';
import { useGitBranchButtonModel } from './useGitBranchButtonModel';

interface GitBranchButtonProps {
  client: ProjectGitApiClient;
  projectId?: string | null;
  projectRoot: string;
  onRepositoryChanged?: () => Promise<void> | void;
}

export const GitBranchButton: React.FC<GitBranchButtonProps> = ({
  client,
  projectId,
  projectRoot,
  onRepositoryChanged,
}) => {
  const model = useGitBranchButtonModel({
    client,
    projectId,
    projectRoot,
    onRepositoryChanged,
  });

  return (
    <div className="relative" ref={model.panelRef}>
      <GitBranchTrigger model={model} />
      {model.open && <GitBranchDropdown model={model} />}
      <GitBranchDialogMounts model={model} />
    </div>
  );
};

export default GitBranchButton;
