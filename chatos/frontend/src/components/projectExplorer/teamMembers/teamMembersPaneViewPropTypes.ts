// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Project } from '../../../types';
import { useTeamMembersPaneSessionResources } from './useTeamMembersPaneSessionResources';
import { useTeamMembersPaneStoreBridge } from './useTeamMembersPaneStoreBridge';

export interface UseTeamMembersPaneViewPropsOptions {
  project: Project;
  store: ReturnType<typeof useTeamMembersPaneStoreBridge>;
  resources: ReturnType<typeof useTeamMembersPaneSessionResources>;
}
