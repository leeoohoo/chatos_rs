import type { ComponentProps } from 'react';

import type { Project } from '../../../types';
import TurnRuntimeContextDrawer from '../../chatInterface/TurnRuntimeContextDrawer';
import { ProjectContactPickerModal } from '../../sessionList/ProjectContactPickerModal';
import TeamMemberWorkspace from './TeamMemberWorkspace';
import TeamMembersSidebar from './TeamMembersSidebar';
import { useTeamMembersPaneSessionResources } from './useTeamMembersPaneSessionResources';
import { useTeamMembersPaneStoreBridge } from './useTeamMembersPaneStoreBridge';
import { useTeamMembersPaneViewProps } from './useTeamMembersPaneViewProps';

interface UseTeamMembersPaneModelOptions {
  project: Project;
}

interface UseTeamMembersPaneModelResult {
  sidebarProps: ComponentProps<typeof TeamMembersSidebar>;
  workspaceProps: ComponentProps<typeof TeamMemberWorkspace>;
  runtimeContextDrawerProps: ComponentProps<typeof TurnRuntimeContextDrawer>;
  memberPickerProps: ComponentProps<typeof ProjectContactPickerModal>;
}

export const useTeamMembersPaneModel = ({
  project,
}: UseTeamMembersPaneModelOptions): UseTeamMembersPaneModelResult => {
  const store = useTeamMembersPaneStoreBridge();
  const resources = useTeamMembersPaneSessionResources({ project, store });
  return useTeamMembersPaneViewProps({ project, store, resources });
};
