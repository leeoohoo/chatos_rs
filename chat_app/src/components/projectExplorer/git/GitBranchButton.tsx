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
  projectRoot: string;
  onRepositoryChanged?: () => Promise<void> | void;
}

export const GitBranchButton: React.FC<GitBranchButtonProps> = ({
  client,
  projectRoot,
  onRepositoryChanged,
}) => {
  const model = useGitBranchButtonModel({ client, projectRoot, onRepositoryChanged });

  return (
    <div className="relative" ref={model.panelRef}>
      <GitBranchTrigger model={model} />
      {model.open && <GitBranchDropdown model={model} />}
      <GitBranchDialogMounts model={model} />
    </div>
  );
};

export default GitBranchButton;
