import { useMemo } from 'react';
import type { ComponentProps } from 'react';

import TeamMembersSidebar from './TeamMembersSidebar';
import type { UseTeamMembersPaneViewPropsOptions } from './teamMembersPaneViewPropTypes';

export const useTeamMembersSidebarProps = ({
  project,
  resources,
}: UseTeamMembersPaneViewPropsOptions): ComponentProps<typeof TeamMembersSidebar> => useMemo(() => ({
  projectName: project.name,
  projectMembersLoading: resources.members.projectMembersLoading,
  projectMembersError: resources.members.projectMembersError,
  memberPickerError: resources.members.memberPickerError,
  projectContacts: resources.members.projectContacts,
  selectedContactId: resources.conversation.selectedContactId,
  switchingContactId: resources.conversation.switchingContactId,
  summaryPaneSessionId: resources.summary.summaryPaneSessionId,
  openingSummaryContactId: resources.conversation.openingSummaryContactId,
  runtimeContextSessionId: resources.runtimeContext.runtimeContextOpen
    ? resources.runtimeContext.runtimeContextSessionId
    : null,
  openingRuntimeContextContactId: resources.runtimeContext.openingRuntimeContextContactId,
  removingContactId: resources.members.removingContactId,
  onOpenAddMember: () => {
    void resources.members.handleOpenAddMember();
  },
  onSelectContact: (contactId) => {
    void resources.conversation.handleSelectContact(contactId);
  },
  onOpenSummary: (contact) => {
    void resources.conversation.handleOpenSummary(contact);
  },
  onOpenRuntimeContext: (contact) => {
    void resources.runtimeContext.handleOpenRuntimeContext(contact);
  },
  onRemoveMember: (contact) => {
    void resources.members.handleRemoveMember(contact);
  },
}), [
  project.name,
  resources.conversation.handleOpenSummary,
  resources.conversation.handleSelectContact,
  resources.conversation.openingSummaryContactId,
  resources.conversation.selectedContactId,
  resources.conversation.switchingContactId,
  resources.members.handleOpenAddMember,
  resources.members.handleRemoveMember,
  resources.members.memberPickerError,
  resources.members.projectContacts,
  resources.members.projectMembersError,
  resources.members.projectMembersLoading,
  resources.members.removingContactId,
  resources.runtimeContext.handleOpenRuntimeContext,
  resources.runtimeContext.openingRuntimeContextContactId,
  resources.runtimeContext.runtimeContextOpen,
  resources.runtimeContext.runtimeContextSessionId,
  resources.summary.summaryPaneSessionId,
]);
