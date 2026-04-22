import { useCallback, useState } from 'react';

import { resolveRemoteConnectionErrorFeedback } from '../../lib/api/remoteConnectionErrors';
import type { RemoteConnection } from '../../types';
import type { HostKeyPolicy, JumpHostMode, RemoteAuthType } from './helpers';
import { buildRemoteConnectionPayload } from './helpers';

interface UseRemoteConnectionFormOptions {
  apiClient: any;
  remoteConnections: RemoteConnection[];
  createRemoteConnection: (payload: any) => Promise<any>;
  updateRemoteConnection: (id: string, payload: any) => Promise<any>;
}

export const useRemoteConnectionForm = ({
  apiClient,
  remoteConnections,
  createRemoteConnection,
  updateRemoteConnection,
}: UseRemoteConnectionFormOptions) => {
  const [remoteModalOpen, setRemoteModalOpen] = useState(false);
  const [remoteName, setRemoteName] = useState('');
  const [remoteHost, setRemoteHost] = useState('');
  const [remotePort, setRemotePort] = useState('22');
  const [remoteUsername, setRemoteUsername] = useState('');
  const [remoteAuthType, setRemoteAuthType] = useState<RemoteAuthType>('private_key');
  const [remotePassword, setRemotePassword] = useState('');
  const [remotePrivateKeyPath, setRemotePrivateKeyPath] = useState('');
  const [remoteCertificatePath, setRemoteCertificatePath] = useState('');
  const [remoteDefaultPath, setRemoteDefaultPath] = useState('');
  const [remoteHostKeyPolicy, setRemoteHostKeyPolicy] = useState<HostKeyPolicy>('strict');
  const [remoteJumpEnabled, setRemoteJumpEnabled] = useState(false);
  const [remoteJumpMode, setRemoteJumpMode] = useState<JumpHostMode>('manual');
  const [remoteJumpConnectionId, setRemoteJumpConnectionId] = useState('');
  const [remoteJumpHost, setRemoteJumpHost] = useState('');
  const [remoteJumpPort, setRemoteJumpPort] = useState('22');
  const [remoteJumpUsername, setRemoteJumpUsername] = useState('');
  const [remoteJumpPrivateKeyPath, setRemoteJumpPrivateKeyPath] = useState('');
  const [remoteJumpCertificatePath, setRemoteJumpCertificatePath] = useState('');
  const [remoteJumpPassword, setRemoteJumpPassword] = useState('');
  const [remoteError, setRemoteError] = useState<string | null>(null);
  const [remoteErrorAction, setRemoteErrorAction] = useState<string | null>(null);
  const [remoteSuccess, setRemoteSuccess] = useState<string | null>(null);
  const [remoteTesting, setRemoteTesting] = useState(false);
  const [remoteSaving, setRemoteSaving] = useState(false);
  const [editingRemoteConnectionId, setEditingRemoteConnectionId] = useState<string | null>(null);
  const [remoteVerificationModalOpen, setRemoteVerificationModalOpen] = useState(false);
  const [remoteVerificationPrompt, setRemoteVerificationPrompt] = useState('');
  const [remoteVerificationCode, setRemoteVerificationCode] = useState('');
  const [pendingVerificationDraftPayload, setPendingVerificationDraftPayload] = useState<any | null>(null);
  const [pendingVerificationConnectionId, setPendingVerificationConnectionId] = useState<string | null>(null);

  const isSecondFactorRequired = useCallback((error: any) => (
    typeof error?.code === 'string' && error.code === 'second_factor_required'
  ), []);

  const extractSecondFactorPrompt = useCallback((error: any) => {
    const prompt = error?.payload?.challenge_prompt;
    if (typeof prompt === 'string' && prompt.trim()) {
      return prompt.trim();
    }
    return '请输入短信验证码或 OTP';
  }, []);

  const applyRemoteErrorFeedback = useCallback((error: unknown, fallback: string) => {
    const feedback = resolveRemoteConnectionErrorFeedback(error, fallback);
    setRemoteError(feedback.message);
    setRemoteErrorAction(feedback.action ?? null);
  }, []);

  const handleRemoteJumpEnabledChange = useCallback((enabled: boolean) => {
    setRemoteJumpEnabled(enabled);
    if (enabled && remoteConnections.length > 0 && !remoteJumpConnectionId.trim()) {
      setRemoteJumpMode('existing');
    }
  }, [remoteConnections.length, remoteJumpConnectionId]);

  const openRemoteModal = useCallback(() => {
    setEditingRemoteConnectionId(null);
    setRemoteName('');
    setRemoteHost('');
    setRemotePort('22');
    setRemoteUsername('');
    setRemoteAuthType('private_key');
    setRemotePassword('');
    setRemotePrivateKeyPath('');
    setRemoteCertificatePath('');
    setRemoteDefaultPath('');
    setRemoteHostKeyPolicy('strict');
    setRemoteJumpEnabled(false);
    setRemoteJumpMode('manual');
    setRemoteJumpConnectionId('');
    setRemoteJumpHost('');
    setRemoteJumpPort('22');
    setRemoteJumpUsername('');
    setRemoteJumpPrivateKeyPath('');
    setRemoteJumpCertificatePath('');
    setRemoteJumpPassword('');
    setRemoteError(null);
    setRemoteErrorAction(null);
    setRemoteSuccess(null);
    setRemoteTesting(false);
    setRemoteSaving(false);
    setRemoteVerificationModalOpen(false);
    setRemoteVerificationPrompt('');
    setRemoteVerificationCode('');
    setPendingVerificationDraftPayload(null);
    setPendingVerificationConnectionId(null);
    setRemoteModalOpen(true);
  }, []);

  const openEditRemoteModal = useCallback((connection: RemoteConnection) => {
    setEditingRemoteConnectionId(connection.id);
    setRemoteName(connection.name || '');
    setRemoteHost(connection.host || '');
    setRemotePort(String(connection.port || 22));
    setRemoteUsername(connection.username || '');
    setRemoteAuthType(connection.authType || 'private_key');
    setRemotePassword(connection.password || '');
    setRemotePrivateKeyPath(connection.privateKeyPath || '');
    setRemoteCertificatePath(connection.certificatePath || '');
    setRemoteDefaultPath(connection.defaultRemotePath || '');
    setRemoteHostKeyPolicy(connection.hostKeyPolicy || 'strict');
    setRemoteJumpEnabled(Boolean(connection.jumpEnabled));
    setRemoteJumpMode(connection.jumpConnectionId ? 'existing' : 'manual');
    setRemoteJumpConnectionId(connection.jumpConnectionId || '');
    setRemoteJumpHost(connection.jumpHost || '');
    setRemoteJumpPort(String(connection.jumpPort || 22));
    setRemoteJumpUsername(connection.jumpUsername || '');
    setRemoteJumpPrivateKeyPath(connection.jumpPrivateKeyPath || '');
    setRemoteJumpCertificatePath(connection.jumpCertificatePath || '');
    setRemoteJumpPassword(connection.jumpPassword || '');
    setRemoteError(null);
    setRemoteErrorAction(null);
    setRemoteSuccess(null);
    setRemoteTesting(false);
    setRemoteSaving(false);
    setRemoteVerificationModalOpen(false);
    setRemoteVerificationPrompt('');
    setRemoteVerificationCode('');
    setPendingVerificationDraftPayload(null);
    setPendingVerificationConnectionId(null);
    setRemoteModalOpen(true);
  }, []);

  const handleTestRemoteConnection = useCallback(async () => {
    const built = buildRemoteConnectionPayload({
      name: remoteName,
      host: remoteHost,
      port: remotePort,
      username: remoteUsername,
      authType: remoteAuthType,
      password: remotePassword,
      privateKeyPath: remotePrivateKeyPath,
      certificatePath: remoteCertificatePath,
      defaultPath: remoteDefaultPath,
      hostKeyPolicy: remoteHostKeyPolicy,
      jumpEnabled: remoteJumpEnabled,
      jumpMode: remoteJumpMode,
      jumpConnectionId: remoteJumpConnectionId,
      jumpHost: remoteJumpHost,
      jumpPort: remoteJumpPort,
      jumpUsername: remoteJumpUsername,
      jumpPrivateKeyPath: remoteJumpPrivateKeyPath,
      jumpCertificatePath: remoteJumpCertificatePath,
      jumpPassword: remoteJumpPassword,
    }, remoteConnections, editingRemoteConnectionId);
    if ('error' in built) {
      setRemoteError(built.error);
      setRemoteErrorAction(null);
      setRemoteSuccess(null);
      return;
    }
    setRemoteTesting(true);
    setRemoteError(null);
    setRemoteErrorAction(null);
    setRemoteSuccess(null);
    try {
      const result = await apiClient.testRemoteConnectionDraft(built.payload);
      const remoteHostName = result?.remote_host ? ` (${result.remote_host})` : '';
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
    applyRemoteErrorFeedback,
    apiClient,
    editingRemoteConnectionId,
    extractSecondFactorPrompt,
    isSecondFactorRequired,
    remoteName,
    remoteHost,
    remotePort,
    remoteUsername,
    remoteAuthType,
    remotePassword,
    remotePrivateKeyPath,
    remoteCertificatePath,
    remoteDefaultPath,
    remoteHostKeyPolicy,
    remoteJumpEnabled,
    remoteJumpMode,
    remoteJumpConnectionId,
    remoteJumpHost,
    remoteJumpPort,
    remoteJumpUsername,
    remoteJumpPrivateKeyPath,
    remoteJumpCertificatePath,
    remoteJumpPassword,
    remoteConnections,
  ]);

  const handleSaveRemoteConnection = useCallback(async () => {
    const built = buildRemoteConnectionPayload({
      name: remoteName,
      host: remoteHost,
      port: remotePort,
      username: remoteUsername,
      authType: remoteAuthType,
      password: remotePassword,
      privateKeyPath: remotePrivateKeyPath,
      certificatePath: remoteCertificatePath,
      defaultPath: remoteDefaultPath,
      hostKeyPolicy: remoteHostKeyPolicy,
      jumpEnabled: remoteJumpEnabled,
      jumpMode: remoteJumpMode,
      jumpConnectionId: remoteJumpConnectionId,
      jumpHost: remoteJumpHost,
      jumpPort: remoteJumpPort,
      jumpUsername: remoteJumpUsername,
      jumpPrivateKeyPath: remoteJumpPrivateKeyPath,
      jumpCertificatePath: remoteJumpCertificatePath,
      jumpPassword: remoteJumpPassword,
    }, remoteConnections, editingRemoteConnectionId);
    if ('error' in built) {
      setRemoteError(built.error);
      setRemoteErrorAction(null);
      setRemoteSuccess(null);
      return;
    }
    setRemoteSaving(true);
    setRemoteError(null);
    setRemoteErrorAction(null);
    setRemoteSuccess(null);
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
    createRemoteConnection,
    editingRemoteConnectionId,
    remoteName,
    remoteHost,
    remotePort,
    remoteUsername,
    remoteAuthType,
    remotePassword,
    remotePrivateKeyPath,
    remoteCertificatePath,
    remoteDefaultPath,
    remoteHostKeyPolicy,
    remoteJumpEnabled,
    remoteJumpMode,
    remoteJumpConnectionId,
    remoteJumpHost,
    remoteJumpPort,
    remoteJumpUsername,
    remoteJumpPrivateKeyPath,
    remoteJumpCertificatePath,
    remoteJumpPassword,
    remoteConnections,
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
  }, [apiClient, applyRemoteErrorFeedback, extractSecondFactorPrompt, isSecondFactorRequired]);

  const handleSubmitRemoteVerification = useCallback(async () => {
    const code = remoteVerificationCode.trim();
    if (!code) {
      setRemoteError('请输入验证码');
      return;
    }
    setRemoteTesting(true);
    setRemoteError(null);
    setRemoteErrorAction(null);
    setRemoteSuccess(null);
    try {
      if (pendingVerificationDraftPayload) {
        const result = await apiClient.testRemoteConnectionDraft(pendingVerificationDraftPayload, code);
        const remoteHostName = result?.remote_host ? ` (${result.remote_host})` : '';
        setRemoteSuccess(`连接测试成功${remoteHostName}`);
      } else if (pendingVerificationConnectionId) {
        await apiClient.testRemoteConnection(pendingVerificationConnectionId, code);
        setRemoteSuccess('连接测试成功');
      } else {
        throw new Error('验证码上下文已失效，请重新发起连接测试');
      }
      setRemoteVerificationModalOpen(false);
      setPendingVerificationDraftPayload(null);
      setPendingVerificationConnectionId(null);
      setRemoteVerificationCode('');
      setRemoteVerificationPrompt('');
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
    extractSecondFactorPrompt,
    isSecondFactorRequired,
    pendingVerificationConnectionId,
    pendingVerificationDraftPayload,
    remoteVerificationCode,
  ]);

  return {
    remoteModalOpen,
    setRemoteModalOpen,
    remoteName,
    setRemoteName,
    remoteHost,
    setRemoteHost,
    remotePort,
    setRemotePort,
    remoteUsername,
    setRemoteUsername,
    remoteAuthType,
    setRemoteAuthType,
    remotePassword,
    setRemotePassword,
    remotePrivateKeyPath,
    setRemotePrivateKeyPath,
    remoteCertificatePath,
    setRemoteCertificatePath,
    remoteDefaultPath,
    setRemoteDefaultPath,
    remoteHostKeyPolicy,
    setRemoteHostKeyPolicy,
    remoteJumpEnabled,
    setRemoteJumpEnabled: handleRemoteJumpEnabledChange,
    remoteJumpMode,
    setRemoteJumpMode,
    remoteJumpConnectionId,
    setRemoteJumpConnectionId,
    remoteJumpHost,
    setRemoteJumpHost,
    remoteJumpPort,
    setRemoteJumpPort,
    remoteJumpUsername,
    setRemoteJumpUsername,
    remoteJumpPrivateKeyPath,
    setRemoteJumpPrivateKeyPath,
    remoteJumpCertificatePath,
    setRemoteJumpCertificatePath,
    remoteJumpPassword,
    setRemoteJumpPassword,
    remoteError,
    remoteErrorAction,
    setRemoteError,
    setRemoteErrorAction,
    remoteSuccess,
    setRemoteSuccess,
    remoteTesting,
    setRemoteTesting,
    remoteSaving,
    setRemoteSaving,
    editingRemoteConnectionId,
    setEditingRemoteConnectionId,
    remoteVerificationModalOpen,
    setRemoteVerificationModalOpen,
    remoteVerificationPrompt,
    setRemoteVerificationPrompt,
    remoteVerificationCode,
    setRemoteVerificationCode,
    openRemoteModal,
    openEditRemoteModal,
    handleTestRemoteConnection,
    handleSaveRemoteConnection,
    handleQuickTestRemoteConnection,
    handleSubmitRemoteVerification,
  };
};
