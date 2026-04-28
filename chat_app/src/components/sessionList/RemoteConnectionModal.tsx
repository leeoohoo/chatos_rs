import type { FC } from 'react';
import RemoteVerificationModal from '../remote/RemoteVerificationModal';
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
  if (!isOpen) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div className="relative bg-card border border-border rounded-lg shadow-xl w-[620px] p-6 max-h-[85vh] overflow-y-auto">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold text-foreground">
            {editingRemoteConnection ? '编辑远端连接' : '新增远端连接'}
          </h3>
          <button
            onClick={onClose}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
          >
            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
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
      </div>
      <RemoteVerificationModal
        isOpen={remoteVerificationModalOpen}
        prompt={remoteVerificationPrompt}
        code={remoteVerificationCode}
        submitting={remoteTesting}
        onCodeChange={onRemoteVerificationCodeChange}
        onClose={onRemoteVerificationClose}
        onSubmit={onRemoteVerificationSubmit}
      />
    </div>
  );
};
