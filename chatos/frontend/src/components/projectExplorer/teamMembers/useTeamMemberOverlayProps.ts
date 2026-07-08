// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import type { ComponentProps } from 'react';

import TurnRuntimeContextDrawer from '../../chatInterface/TurnRuntimeContextDrawer';
import { ProjectContactPickerModal } from '../../sessionList/ProjectContactPickerModal';
import type { UseTeamMembersPaneViewPropsOptions } from './teamMembersPaneViewPropTypes';

export const useTeamMemberRuntimeContextDrawerProps = ({
  resources,
}: UseTeamMembersPaneViewPropsOptions): ComponentProps<typeof TurnRuntimeContextDrawer> => useMemo(() => ({
  open: resources.runtimeContext.runtimeContextOpen,
  sessionId: resources.runtimeContext.runtimeContextSessionId,
  loading: resources.runtimeContext.runtimeContextLoading,
  error: resources.runtimeContext.runtimeContextError,
  data: resources.runtimeContext.runtimeContextData,
  onRefresh: resources.runtimeContext.handleRefreshRuntimeContext,
  onClose: () => {
    resources.runtimeContext.setRuntimeContextOpen(false);
  },
}), [
  resources.runtimeContext.handleRefreshRuntimeContext,
  resources.runtimeContext.runtimeContextData,
  resources.runtimeContext.runtimeContextError,
  resources.runtimeContext.runtimeContextLoading,
  resources.runtimeContext.runtimeContextOpen,
  resources.runtimeContext.runtimeContextSessionId,
  resources.runtimeContext.setRuntimeContextOpen,
]);

export const useTeamMemberPickerProps = ({
  project,
  resources,
}: UseTeamMembersPaneViewPropsOptions): ComponentProps<typeof ProjectContactPickerModal> => useMemo(() => ({
  isOpen: resources.members.memberPickerOpen,
  projectName: project.name,
  contacts: resources.members.projectContactsOptions,
  disabledContactIds: Array.from(resources.members.projectContactIdSet),
  selectedContactId: resources.members.memberPickerSelectedId,
  error: resources.members.memberPickerError,
  onClose: resources.members.closeMemberPicker,
  onSelectedContactChange: resources.members.selectMemberPickerContact,
  onConfirm: () => {
    void resources.members.handleConfirmAddMember();
  },
}), [
  project.name,
  resources.members.closeMemberPicker,
  resources.members.handleConfirmAddMember,
  resources.members.memberPickerError,
  resources.members.memberPickerOpen,
  resources.members.memberPickerSelectedId,
  resources.members.projectContactIdSet,
  resources.members.projectContactsOptions,
  resources.members.selectMemberPickerContact,
]);
