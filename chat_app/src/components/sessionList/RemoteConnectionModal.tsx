import type { FC } from 'react';
import RemoteVerificationModal from '../remote/RemoteVerificationModal';
import ManagerFormDialog from '../ui/ManagerFormDialog';
import { AuthSection } from './remoteConnectionModal/AuthSection';
import { ConnectionBasicsSection } from './remoteConnectionModal/ConnectionBasicsSection';
import { JumpHostSection } from './remoteConnectionModal/JumpHostSection';
import { RemoteConnectionModalFooter } from './remoteConnectionModal/RemoteConnectionModalFooter';
import { RemoteConnectionModalMessages } from './remoteConnectionModal/RemoteConnectionModalMessages';
import type { RemoteConnectionModalProps } from './remoteConnectionModal/types';

export const RemoteConnectionModal: FC<RemoteConnectionModalProps> = ({
  isOpen,
  editingRemoteConnection,
  editingRemoteConnectionId,
  remoteConnections,
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
  remoteError,
  remoteErrorAction,
  remoteSuccess,
  remoteTesting,
  remoteSaving,
  remoteVerificationModalOpen,
  remoteVerificationPrompt,
  remoteVerificationCode,
  onClose,
  onRemoteNameChange,
  onRemoteHostChange,
  onRemotePortChange,
  onRemoteUsernameChange,
  onRemoteAuthTypeChange,
  onRemotePasswordChange,
  onRemotePrivateKeyPathChange,
  onRemoteCertificatePathChange,
  onRemoteDefaultPathChange,
  onRemoteHostKeyPolicyChange,
  onRemoteJumpEnabledChange,
  onRemoteJumpModeChange,
  onRemoteJumpConnectionIdChange,
  onRemoteJumpHostChange,
  onRemoteJumpPortChange,
  onRemoteJumpUsernameChange,
  onRemoteJumpPrivateKeyPathChange,
  onRemoteJumpCertificatePathChange,
  onRemoteJumpPasswordChange,
  onRemoteVerificationCodeChange,
  onRemoteVerificationClose,
  onRemoteVerificationSubmit,
  onOpenKeyFilePicker,
  onTest,
  onSave,
}) => {
  return (
    <>
      <ManagerFormDialog
        open={isOpen}
        title={editingRemoteConnection ? '编辑远端连接' : '新增远端连接'}
        widthClassName="max-w-3xl"
        onClose={onClose}
      >
        <div className="space-y-4">
          <ConnectionBasicsSection
            remoteName={remoteName}
            remoteHost={remoteHost}
            remotePort={remotePort}
            remoteUsername={remoteUsername}
            remoteHostKeyPolicy={remoteHostKeyPolicy}
            onRemoteNameChange={onRemoteNameChange}
            onRemoteHostChange={onRemoteHostChange}
            onRemotePortChange={onRemotePortChange}
            onRemoteUsernameChange={onRemoteUsernameChange}
            onRemoteHostKeyPolicyChange={onRemoteHostKeyPolicyChange}
          />

          <AuthSection
            remoteAuthType={remoteAuthType}
            remotePassword={remotePassword}
            remotePrivateKeyPath={remotePrivateKeyPath}
            remoteCertificatePath={remoteCertificatePath}
            remoteDefaultPath={remoteDefaultPath}
            onRemoteAuthTypeChange={onRemoteAuthTypeChange}
            onRemotePasswordChange={onRemotePasswordChange}
            onRemotePrivateKeyPathChange={onRemotePrivateKeyPathChange}
            onRemoteCertificatePathChange={onRemoteCertificatePathChange}
            onRemoteDefaultPathChange={onRemoteDefaultPathChange}
            onOpenKeyFilePicker={onOpenKeyFilePicker}
          />

          <JumpHostSection
            editingRemoteConnectionId={editingRemoteConnectionId}
            remoteConnections={remoteConnections}
            remoteJumpEnabled={remoteJumpEnabled}
            remoteJumpMode={remoteJumpMode}
            remoteJumpConnectionId={remoteJumpConnectionId}
            remoteJumpHost={remoteJumpHost}
            remoteJumpPort={remoteJumpPort}
            remoteJumpUsername={remoteJumpUsername}
            remoteJumpPrivateKeyPath={remoteJumpPrivateKeyPath}
            remoteJumpCertificatePath={remoteJumpCertificatePath}
            remoteJumpPassword={remoteJumpPassword}
            onRemoteJumpEnabledChange={onRemoteJumpEnabledChange}
            onRemoteJumpModeChange={onRemoteJumpModeChange}
            onRemoteJumpConnectionIdChange={onRemoteJumpConnectionIdChange}
            onRemoteJumpHostChange={onRemoteJumpHostChange}
            onRemoteJumpPortChange={onRemoteJumpPortChange}
            onRemoteJumpUsernameChange={onRemoteJumpUsernameChange}
            onRemoteJumpPrivateKeyPathChange={onRemoteJumpPrivateKeyPathChange}
            onRemoteJumpCertificatePathChange={onRemoteJumpCertificatePathChange}
            onRemoteJumpPasswordChange={onRemoteJumpPasswordChange}
            onOpenKeyFilePicker={onOpenKeyFilePicker}
          />

          <RemoteConnectionModalMessages
            remoteError={remoteError}
            remoteErrorAction={remoteErrorAction}
            remoteSuccess={remoteSuccess}
          />
        </div>
        <RemoteConnectionModalFooter
          editingRemoteConnection={editingRemoteConnection}
          remoteTesting={remoteTesting}
          remoteSaving={remoteSaving}
          onClose={onClose}
          onTest={onTest}
          onSave={onSave}
        />
      </ManagerFormDialog>
      <RemoteVerificationModal
        isOpen={remoteVerificationModalOpen}
        prompt={remoteVerificationPrompt}
        code={remoteVerificationCode}
        submitting={remoteTesting}
        onCodeChange={onRemoteVerificationCodeChange}
        onClose={onRemoteVerificationClose}
        onSubmit={onRemoteVerificationSubmit}
      />
    </>
  );
};
