import type { Project } from '../../../types';
import { useTeamMembersPaneSessionResources } from './useTeamMembersPaneSessionResources';
import { useTeamMembersPaneStoreBridge } from './useTeamMembersPaneStoreBridge';

export interface UseTeamMembersPaneViewPropsOptions {
  project: Project;
  store: ReturnType<typeof useTeamMembersPaneStoreBridge>;
  resources: ReturnType<typeof useTeamMembersPaneSessionResources>;
}
