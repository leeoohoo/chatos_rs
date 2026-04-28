import React from 'react';

import { cn } from '../../lib/utils';
import type { Project } from '../../types';
import TurnRuntimeContextDrawer from '../chatInterface/TurnRuntimeContextDrawer';
import { ProjectContactPickerModal } from '../sessionList/ProjectContactPickerModal';
import TeamMembersSidebar from './teamMembers/TeamMembersSidebar';
import TeamMemberWorkspace from './teamMembers/TeamMemberWorkspace';
import { useTeamMembersPaneModel } from './teamMembers/useTeamMembersPaneModel';

interface TeamMembersPaneProps {
  project: Project;
  className?: string;
}

const TeamMembersPane: React.FC<TeamMembersPaneProps> = ({ project, className }) => {
  const {
    sidebarProps,
    workspaceProps,
    runtimeContextDrawerProps,
    memberPickerProps,
  } = useTeamMembersPaneModel({ project });

  if (!project) {
    return (
      <div className={cn('flex items-center justify-center h-full text-muted-foreground', className)}>
        请选择一个项目
      </div>
    );
  }

  return (
    <div className={cn('flex h-full overflow-hidden', className)}>
      <TeamMembersSidebar {...sidebarProps} />
      <TeamMemberWorkspace {...workspaceProps} />
      <TurnRuntimeContextDrawer {...runtimeContextDrawerProps} />
      <ProjectContactPickerModal {...memberPickerProps} />
    </div>
  );
};

export default TeamMembersPane;
