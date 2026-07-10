// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
  enabled?: boolean;
  readOnly?: boolean;
  onRepositoryChanged?: () => Promise<void> | void;
  onRepositorySelectionChange?: (repoRoot: string | null) => Promise<void> | void;
}

export const GitBranchButton: React.FC<GitBranchButtonProps> = ({
  client,
  projectId,
  projectRoot,
  enabled = true,
  readOnly = false,
  onRepositoryChanged,
  onRepositorySelectionChange,
}) => {
  const model = useGitBranchButtonModel({
    client,
    projectId,
    projectRoot,
    enabled,
    readOnly,
    onRepositoryChanged,
    onRepositorySelectionChange,
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
