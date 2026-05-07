import type { UseRemoteConnectionFormOptions } from './remoteConnectionForm/types';
import { useRemoteConnectionFormActions } from './remoteConnectionForm/useRemoteConnectionFormActions';
import { useRemoteConnectionFormState } from './remoteConnectionForm/useRemoteConnectionFormState';

export const useRemoteConnectionForm = ({
  apiClient,
  remoteConnections,
  createRemoteConnection,
  updateRemoteConnection,
}: UseRemoteConnectionFormOptions) => {
  const form = useRemoteConnectionFormState();
  const actions = useRemoteConnectionFormActions({
    apiClient,
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
