// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';

import { resolveRemoteConnectionErrorFeedback } from '../../../lib/api/remoteConnectionErrors';
import type { RemoteConnection } from '../../../types';
import { buildRemoteConnectionPayload, translateSessionListMessage } from '../helpers';
import type {
  RemoteConnectionTestResult,
  UseRemoteConnectionFormOptions,
} from './types';
import type { useRemoteConnectionFormState } from './useRemoteConnectionFormState';
import {
  extractSecondFactorPrompt,
  isSecondFactorRequired,
  readRemoteHostName,
} from './verification';

type RemoteConnectionFormState = ReturnType<typeof useRemoteConnectionFormState>;

interface UseRemoteConnectionFormActionsOptions extends UseRemoteConnectionFormOptions {
  form: RemoteConnectionFormState;
}

export const useRemoteConnectionFormActions = ({
  apiClient,
  t,
  remoteConnections,
  createRemoteConnection,
  updateRemoteConnection,
  form,
}: UseRemoteConnectionFormActionsOptions) => {
  const {
    clearFeedback,
    clearVerificationState,
    editingRemoteConnectionId,
    hydrateForEdit,
    pendingVerificationConnectionId,
    pendingVerificationDraftPayload,
    readCurrentFormValues,
    remoteJumpConnectionId,
    remoteVerificationCode,
    resetForCreate,
    setPendingVerificationConnectionId,
    setPendingVerificationDraftPayload,
    setRemoteError,
    setRemoteErrorAction,
    setRemoteJumpEnabled,
    setRemoteJumpMode,
    setRemoteModalOpen,
    setRemoteSaving,
    setRemoteSuccess,
    setRemoteTesting,
    setRemoteVerificationCode,
    setRemoteVerificationModalOpen,
    setRemoteVerificationPrompt,
  } = form;

  const applyRemoteErrorFeedback = useCallback((error: unknown, fallback: string) => {
    const feedback = resolveRemoteConnectionErrorFeedback(error, fallback);
    setRemoteError(feedback.message);
    setRemoteErrorAction(feedback.action ?? null);
  }, [setRemoteError, setRemoteErrorAction]);

  const handleRemoteJumpEnabledChange = useCallback((enabled: boolean) => {
    setRemoteJumpEnabled(enabled);
    if (enabled && remoteConnections.length > 0 && !remoteJumpConnectionId.trim()) {
      setRemoteJumpMode('existing');
    }
  }, [remoteConnections.length, remoteJumpConnectionId, setRemoteJumpEnabled, setRemoteJumpMode]);

  const openRemoteModal = useCallback(() => {
    resetForCreate();
    setRemoteModalOpen(true);
  }, [resetForCreate, setRemoteModalOpen]);

  const openEditRemoteModal = useCallback((connection: RemoteConnection) => {
    hydrateForEdit(connection);
    setRemoteModalOpen(true);
  }, [hydrateForEdit, setRemoteModalOpen]);

  const handleTestRemoteConnection = useCallback(async () => {
    const built = buildRemoteConnectionPayload(
      readCurrentFormValues(),
      remoteConnections,
      editingRemoteConnectionId,
      t,
    );
    if ('error' in built) {
      setRemoteError(built.error);
      setRemoteErrorAction(null);
      setRemoteSuccess(null);
      return;
    }

    setRemoteTesting(true);
    clearFeedback();
    try {
      const result = await apiClient.testRemoteConnectionDraft(built.payload) as RemoteConnectionTestResult;
      const remoteHostName = readRemoteHostName(result);
      setRemoteSuccess(translateSessionListMessage(t, 'remoteConnection.success.test', { host: remoteHostName }));
      setRemoteErrorAction(null);
    } catch (error) {
      if (isSecondFactorRequired(error)) {
        setPendingVerificationDraftPayload(built.payload);
        setPendingVerificationConnectionId(null);
        setRemoteVerificationPrompt(extractSecondFactorPrompt(
          error,
          translateSessionListMessage(t, 'remoteConnection.verificationPrompt'),
        ));
        setRemoteVerificationCode('');
        setRemoteVerificationModalOpen(true);
        setRemoteError(null);
        setRemoteErrorAction(null);
        return;
      }
      applyRemoteErrorFeedback(error, translateSessionListMessage(t, 'remoteConnection.error.testFailed'));
    } finally {
      setRemoteTesting(false);
    }
  }, [
    apiClient,
    applyRemoteErrorFeedback,
    clearFeedback,
    editingRemoteConnectionId,
    readCurrentFormValues,
    remoteConnections,
    setPendingVerificationConnectionId,
    setPendingVerificationDraftPayload,
    setRemoteError,
    setRemoteErrorAction,
    setRemoteSuccess,
    setRemoteTesting,
    setRemoteVerificationCode,
    setRemoteVerificationModalOpen,
    setRemoteVerificationPrompt,
    t,
  ]);

  const handleSaveRemoteConnection = useCallback(async () => {
    const built = buildRemoteConnectionPayload(
      readCurrentFormValues(),
      remoteConnections,
      editingRemoteConnectionId,
      t,
    );
    if ('error' in built) {
      setRemoteError(built.error);
      setRemoteErrorAction(null);
      setRemoteSuccess(null);
      return;
    }

    setRemoteSaving(true);
    clearFeedback();
    try {
      if (editingRemoteConnectionId) {
        const updated = await updateRemoteConnection(editingRemoteConnectionId, built.payload);
        if (!updated) {
          throw new Error(translateSessionListMessage(t, 'remoteConnection.error.updateFailed'));
        }
      } else {
        await createRemoteConnection(built.payload);
      }
      setRemoteModalOpen(false);
      setRemoteErrorAction(null);
    } catch (error) {
      applyRemoteErrorFeedback(
        error,
        editingRemoteConnectionId
          ? translateSessionListMessage(t, 'remoteConnection.error.updateFailed')
          : translateSessionListMessage(t, 'remoteConnection.error.createFailed'),
      );
    } finally {
      setRemoteSaving(false);
    }
  }, [
    applyRemoteErrorFeedback,
    clearFeedback,
    createRemoteConnection,
    editingRemoteConnectionId,
    readCurrentFormValues,
    remoteConnections,
    setRemoteError,
    setRemoteErrorAction,
    setRemoteModalOpen,
    setRemoteSaving,
    setRemoteSuccess,
    t,
    updateRemoteConnection,
  ]);

  const handleQuickTestRemoteConnection = useCallback(async (connection: RemoteConnection) => {
    try {
      await apiClient.testRemoteConnection(connection.id);
      setRemoteSuccess(translateSessionListMessage(t, 'remoteConnection.success.quickTest', { name: connection.name }));
      setRemoteError(null);
      setRemoteErrorAction(null);
    } catch (error) {
      if (isSecondFactorRequired(error)) {
        setPendingVerificationDraftPayload(null);
        setPendingVerificationConnectionId(connection.id);
        setRemoteVerificationPrompt(extractSecondFactorPrompt(
          error,
          translateSessionListMessage(t, 'remoteConnection.verificationPrompt'),
        ));
        setRemoteVerificationCode('');
        setRemoteVerificationModalOpen(true);
        setRemoteError(null);
        setRemoteErrorAction(null);
        return;
      }
      applyRemoteErrorFeedback(error, translateSessionListMessage(t, 'remoteConnection.error.testFailed'));
    }
  }, [
    apiClient,
    applyRemoteErrorFeedback,
    setPendingVerificationConnectionId,
    setPendingVerificationDraftPayload,
    setRemoteError,
    setRemoteErrorAction,
    setRemoteSuccess,
    setRemoteVerificationCode,
    setRemoteVerificationModalOpen,
    setRemoteVerificationPrompt,
    t,
  ]);

  const handleSubmitRemoteVerification = useCallback(async () => {
    const code = remoteVerificationCode.trim();
    if (!code) {
      setRemoteError(translateSessionListMessage(t, 'remoteConnection.error.codeRequired'));
      return;
    }

    setRemoteTesting(true);
    clearFeedback();
    try {
      if (pendingVerificationDraftPayload) {
        const result = await apiClient.testRemoteConnectionDraft(
          pendingVerificationDraftPayload,
          code,
        ) as RemoteConnectionTestResult;
        const remoteHostName = readRemoteHostName(result);
        setRemoteSuccess(translateSessionListMessage(t, 'remoteConnection.success.test', { host: remoteHostName }));
      } else if (pendingVerificationConnectionId) {
        await apiClient.testRemoteConnection(pendingVerificationConnectionId, code);
        setRemoteSuccess(translateSessionListMessage(t, 'remoteConnection.success.test', { host: '' }));
      } else {
        throw new Error(translateSessionListMessage(t, 'remoteConnection.error.verificationExpired'));
      }
      clearVerificationState();
    } catch (error) {
      if (isSecondFactorRequired(error)) {
        setRemoteVerificationPrompt(extractSecondFactorPrompt(
          error,
          translateSessionListMessage(t, 'remoteConnection.verificationPrompt'),
        ));
        setRemoteError(translateSessionListMessage(t, 'remoteConnection.error.codeInvalid'));
        return;
      }
      applyRemoteErrorFeedback(error, translateSessionListMessage(t, 'remoteConnection.error.testFailed'));
      setRemoteVerificationModalOpen(false);
    } finally {
      setRemoteTesting(false);
    }
  }, [
    apiClient,
    applyRemoteErrorFeedback,
    clearFeedback,
    clearVerificationState,
    pendingVerificationConnectionId,
    pendingVerificationDraftPayload,
    remoteVerificationCode,
    setRemoteError,
    setRemoteSuccess,
    setRemoteTesting,
    setRemoteVerificationModalOpen,
    setRemoteVerificationPrompt,
    t,
  ]);

  return {
    handleRemoteJumpEnabledChange,
    openRemoteModal,
    openEditRemoteModal,
    handleTestRemoteConnection,
    handleSaveRemoteConnection,
    handleQuickTestRemoteConnection,
    handleSubmitRemoteVerification,
  };
};
