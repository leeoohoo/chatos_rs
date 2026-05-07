import { useTeamMemberPickerProps, useTeamMemberRuntimeContextDrawerProps } from './useTeamMemberOverlayProps';
import { useTeamMemberWorkspaceProps } from './useTeamMemberWorkspaceProps';
import { useTeamMembersSidebarProps } from './useTeamMembersSidebarProps';
import type { UseTeamMembersPaneViewPropsOptions } from './teamMembersPaneViewPropTypes';

export const useTeamMembersPaneViewProps = (options: UseTeamMembersPaneViewPropsOptions) => {
  const sidebarProps = useTeamMembersSidebarProps(options);
  const workspaceProps = useTeamMemberWorkspaceProps(options);
  const runtimeContextDrawerProps = useTeamMemberRuntimeContextDrawerProps(options);
  const memberPickerProps = useTeamMemberPickerProps(options);

  return {
    sidebarProps,
    workspaceProps,
    runtimeContextDrawerProps,
    memberPickerProps,
  };
};
