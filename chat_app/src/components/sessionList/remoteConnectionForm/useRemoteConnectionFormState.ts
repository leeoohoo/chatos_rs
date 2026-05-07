import { useCallback, useState } from 'react';

import type { RemoteConnectionDraftPayload } from '../../../lib/api/client/types';
import type { RemoteConnection } from '../../../types';
import type {
  HostKeyPolicy,
  JumpHostMode,
  RemoteAuthType,
  RemoteConnectionFormValues,
} from '../helpers';

export const useRemoteConnectionFormState = () => {
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
  const [pendingVerificationDraftPayload, setPendingVerificationDraftPayload] =
    useState<RemoteConnectionDraftPayload | null>(null);
  const [pendingVerificationConnectionId, setPendingVerificationConnectionId] = useState<string | null>(null);

  const clearFeedback = useCallback(() => {
    setRemoteError(null);
    setRemoteErrorAction(null);
    setRemoteSuccess(null);
  }, []);

  const clearVerificationState = useCallback(() => {
    setRemoteVerificationModalOpen(false);
    setRemoteVerificationPrompt('');
    setRemoteVerificationCode('');
    setPendingVerificationDraftPayload(null);
    setPendingVerificationConnectionId(null);
  }, []);

  const resetProgressState = useCallback(() => {
    clearFeedback();
    setRemoteTesting(false);
    setRemoteSaving(false);
    clearVerificationState();
  }, [clearFeedback, clearVerificationState]);

  const resetForCreate = useCallback(() => {
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
    resetProgressState();
  }, [resetProgressState]);

  const hydrateForEdit = useCallback((connection: RemoteConnection) => {
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
    resetProgressState();
  }, [resetProgressState]);

  const readCurrentFormValues = useCallback((): RemoteConnectionFormValues => ({
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
  }), [
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
    setRemoteJumpEnabled,
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
    setRemoteError,
    remoteErrorAction,
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
    pendingVerificationDraftPayload,
    setPendingVerificationDraftPayload,
    pendingVerificationConnectionId,
    setPendingVerificationConnectionId,
    clearFeedback,
    clearVerificationState,
    resetForCreate,
    hydrateForEdit,
    readCurrentFormValues,
  };
};
