// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useProjectExplorerWorkspaceShell } from './useProjectExplorerWorkspaceShell';
import type { ProjectExplorerWorkspaceShellParams } from './workspaceViewTypes';

export const useProjectExplorerWorkspaceViewModel = (
  params: ProjectExplorerWorkspaceShellParams,
) => useProjectExplorerWorkspaceShell(params);
