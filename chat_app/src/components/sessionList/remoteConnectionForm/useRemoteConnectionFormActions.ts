import { useCallback } from 'react';

import { resolveRemoteConnectionErrorFeedback } from '../../../lib/api/remoteConnectionErrors';
import type { RemoteConnection } from '../../../types';
import { buildRemoteConnectionPayload } from '../helpers';
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
      setRemoteSuccess(`连接测试成功${remoteHostName}`);
      setRemoteErrorAction(null);
    } catch (error) {
      if (isSecondFactorRequired(error)) {
        setPendingVerificationDraftPayload(built.payload);
        setPendingVerificationConnectionId(null);
        setRemoteVerificationPrompt(extractSecondFactorPrompt(error));
        setRemoteVerificationCode('');
        setRemoteVerificationModalOpen(true);
        setRemoteError(null);
        setRemoteErrorAction(null);
        return;
      }
      applyRemoteErrorFeedback(error, '连接测试失败');
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
  ]);

  const handleSaveRemoteConnection = useCallback(async () => {
    const built = buildRemoteConnectionPayload(
      readCurrentFormValues(),
      remoteConnections,
      editingRemoteConnectionId,
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
          throw new Error('更新远端连接失败');
        }
      } else {
        await createRemoteConnection(built.payload);
      }
      setRemoteModalOpen(false);
      setRemoteErrorAction(null);
    } catch (error) {
      applyRemoteErrorFeedback(
        error,
        editingRemoteConnectionId ? '更新远端连接失败' : '创建远端连接失败',
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
    updateRemoteConnection,
  ]);

  const handleQuickTestRemoteConnection = useCallback(async (connection: RemoteConnection) => {
    try {
      await apiClient.testRemoteConnection(connection.id);
      setRemoteSuccess(`连接测试成功 (${connection.name})`);
      setRemoteError(null);
      setRemoteErrorAction(null);
    } catch (error) {
      if (isSecondFactorRequired(error)) {
        setPendingVerificationDraftPayload(null);
        setPendingVerificationConnectionId(connection.id);
        setRemoteVerificationPrompt(extractSecondFactorPrompt(error));
        setRemoteVerificationCode('');
        setRemoteVerificationModalOpen(true);
        setRemoteError(null);
        setRemoteErrorAction(null);
        return;
      }
      applyRemoteErrorFeedback(error, '连接测试失败');
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
  ]);

  const handleSubmitRemoteVerification = useCallback(async () => {
    const code = remoteVerificationCode.trim();
    if (!code) {
      setRemoteError('请输入验证码');
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
        setRemoteSuccess(`连接测试成功${remoteHostName}`);
      } else if (pendingVerificationConnectionId) {
        await apiClient.testRemoteConnection(pendingVerificationConnectionId, code);
        setRemoteSuccess('连接测试成功');
      } else {
        throw new Error('验证码上下文已失效，请重新发起连接测试');
      }
      clearVerificationState();
    } catch (error) {
      if (isSecondFactorRequired(error)) {
        setRemoteVerificationPrompt(extractSecondFactorPrompt(error));
        setRemoteError('验证码错误或已过期，请重试');
        return;
      }
      applyRemoteErrorFeedback(error, '连接测试失败');
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
