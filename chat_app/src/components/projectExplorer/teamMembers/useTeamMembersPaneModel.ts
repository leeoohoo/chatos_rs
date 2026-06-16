import type { ComponentProps } from 'react';

import type { Project } from '../../../types';
import TurnRuntimeContextDrawer from '../../chatInterface/TurnRuntimeContextDrawer';
import TeamMemberWorkspace from './TeamMemberWorkspace';
import { useTeamMembersPaneSessionResources } from './useTeamMembersPaneSessionResources';
import { useTeamMembersPaneStoreBridge } from './useTeamMembersPaneStoreBridge';
import { useTeamMemberRuntimeContextDrawerProps } from './useTeamMemberOverlayProps';
import { useTeamMemberWorkspaceProps } from './useTeamMemberWorkspaceProps';

interface UseTeamMembersPaneModelOptions {
  project: Project;
}

interface UseTeamMembersPaneModelResult {
  workspaceProps: ComponentProps<typeof TeamMemberWorkspace>;
  runtimeContextDrawerProps: ComponentProps<typeof TurnRuntimeContextDrawer>;
}

export const useTeamMembersPaneModel = ({
  project,
}: UseTeamMembersPaneModelOptions): UseTeamMembersPaneModelResult => {
  const store = useTeamMembersPaneStoreBridge();
  const resources = useTeamMembersPaneSessionResources({ project, store });
  const options = { project, store, resources };
  const workspaceProps = useTeamMemberWorkspaceProps(options);
  const runtimeContextDrawerProps = useTeamMemberRuntimeContextDrawerProps(options);

  return {
    workspaceProps,
    runtimeContextDrawerProps,
  };
};
