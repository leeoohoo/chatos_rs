// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { WorkspaceTab } from './WorkspaceTabs';

export const resolveVisibleWorkspaceTabs = (): WorkspaceTab[] => (
  ['files', 'team', 'plan', 'settings']
);
