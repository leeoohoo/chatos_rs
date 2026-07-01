// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { UseRemoteConnectionFormOptions } from './remoteConnectionForm/types';
import { useRemoteConnectionFormActions } from './remoteConnectionForm/useRemoteConnectionFormActions';
import { useRemoteConnectionFormState } from './remoteConnectionForm/useRemoteConnectionFormState';

export const useRemoteConnectionForm = ({
  apiClient,
  t,
  remoteConnections,
  createRemoteConnection,
  updateRemoteConnection,
}: UseRemoteConnectionFormOptions) => {
  const form = useRemoteConnectionFormState();
  const actions = useRemoteConnectionFormActions({
    apiClient,
    t,
    remoteConnections,
    createRemoteConnection,
    updateRemoteConnection,
    form,
  });

  return {
    ...form,
    setRemoteJumpEnabled: actions.handleRemoteJumpEnabledChange,
    openRemoteModal: actions.openRemoteModal,
    openEditRemoteModal: actions.openEditRemoteModal,
    handleTestRemoteConnection: actions.handleTestRemoteConnection,
    handleSaveRemoteConnection: actions.handleSaveRemoteConnection,
    handleQuickTestRemoteConnection: actions.handleQuickTestRemoteConnection,
    handleSubmitRemoteVerification: actions.handleSubmitRemoteVerification,
  };
};
