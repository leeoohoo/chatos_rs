// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { WorkspaceTab } from './WorkspaceTabs';

export const resolveVisibleWorkspaceTabs = (
  isCloudProject: boolean,
  sandboxEnabled: boolean,
): WorkspaceTab[] => {
  if (isCloudProject) {
    return ['files', 'team', 'plan', 'sandbox'];
  }
  return sandboxEnabled
    ? ['files', 'team', 'plan', 'settings', 'sandbox']
    : ['files', 'team', 'plan', 'settings'];
};
