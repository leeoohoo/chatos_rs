// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import { useI18n } from '../../../i18n/I18nProvider';
import type { RemoteConnection } from '../../../types';
import type {
  JumpHostMode,
  KeyFilePickerTarget,
} from '../helpers';
import { PasswordField } from './PasswordField';

interface JumpHostSectionProps {
  editingRemoteConnectionId: string | null;
  remoteConnections: RemoteConnection[];
  remoteJumpEnabled: boolean;
  remoteJumpMode: JumpHostMode;
  remoteJumpConnectionId: string;
  remoteJumpHost: string;
  remoteJumpPort: string;
  remoteJumpUsername: string;
  remoteJumpPrivateKeyPath: string;
  remoteJumpCertificatePath: string;
  remoteJumpPassword: string;
  onRemoteJumpEnabledChange: (value: boolean) => void;
  onRemoteJumpModeChange: (value: JumpHostMode) => void;
  onRemoteJumpConnectionIdChange: (value: string) => void;
  onRemoteJumpHostChange: (value: string) => void;
  onRemoteJumpPortChange: (value: string) => void;
  onRemoteJumpUsernameChange: (value: string) => void;
  onRemoteJumpPrivateKeyPathChange: (value: string) => void;
  onRemoteJumpCertificatePathChange: (value: string) => void;
  onRemoteJumpPasswordChange: (value: string) => void;
  onOpenKeyFilePicker: (target: KeyFilePickerTarget) => void;
}

const getConnectionLabel = (connection: RemoteConnection) => {
  const endpoint = `${connection.username}@${connection.host}:${connection.port || 22}`;
  if (connection.name && connection.name !== endpoint) {
    return `${connection.name} (${endpoint})`;
  }
  return endpoint;
};

export const JumpHostSection: FC<JumpHostSectionProps> = ({
  editingRemoteConnectionId,
  remoteConnections,
  remoteJumpEnabled,
  remoteJumpMode,
  remoteJumpConnectionId,
  remoteJumpHost,
  remoteJumpPort,
  remoteJumpUsername,
  remoteJumpPrivateKeyPath,
  remoteJumpCertificatePath,
  remoteJumpPassword,
  onRemoteJumpEnabledChange,
  onRemoteJumpModeChange,
  onRemoteJumpConnectionIdChange,
  onRemoteJumpHostChange,
  onRemoteJumpPortChange,
  onRemoteJumpUsernameChange,
  onRemoteJumpPrivateKeyPathChange,
  onRemoteJumpCertificatePathChange,
  onRemoteJumpPasswordChange,
  onOpenKeyFilePicker,
}) => {
  const { t } = useI18n();
  const availableJumpConnections = remoteConnections.filter(
    (connection) => connection.id !== editingRemoteConnectionId,
  );
  const selectedJumpConnection = availableJumpConnections.find(
    (connection) => connection.id === remoteJumpConnectionId,
  );
  const selectedJumpAuthLabel = selectedJumpConnection?.authType === 'password'
    ? t('remoteConnection.auth.password')
    : selectedJumpConnection?.authType === 'private_key_cert'
      ? t('remoteConnection.auth.privateKeyCert')
      : t('remoteConnection.auth.privateKey');

  return (
    <div className="rounded border border-border p-3 space-y-3">
      <label className="inline-flex items-center gap-2 text-sm text-foreground">
        <input
          type="checkbox"
          checked={remoteJumpEnabled}
          onChange={(e) => onRemoteJumpEnabledChange(e.target.checked)}
        />
        {t('remoteConnection.jump.enable')}
      </label>

      {remoteJumpEnabled && (
        <div className="space-y-3">
          <div>
            <label className="text-sm text-muted-foreground">{t('remoteConnection.jump.source')}</label>
            <div className="mt-1 grid grid-cols-2 gap-2 rounded-lg bg-muted/40 p-1">
              <button
                type="button"
                onClick={() => onRemoteJumpModeChange('existing')}
                className={`px-3 py-2 rounded text-sm transition-colors ${
                  remoteJumpMode === 'existing'
                    ? 'bg-background text-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                {t('remoteConnection.jump.existing')}
              </button>
              <button
                type="button"
                onClick={() => onRemoteJumpModeChange('manual')}
                className={`px-3 py-2 rounded text-sm transition-colors ${
                  remoteJumpMode === 'manual'
                    ? 'bg-background text-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                {t('remoteConnection.jump.manual')}
              </button>
            </div>
          </div>

          {remoteJumpMode === 'existing' ? (
            <div className="space-y-2">
              <div>
                <label className="text-sm text-muted-foreground">{t('remoteConnection.jump.existingConnection')}</label>
                <select
                  value={remoteJumpConnectionId}
                  onChange={(e) => onRemoteJumpConnectionIdChange(e.target.value)}
                  disabled={availableJumpConnections.length === 0}
                  className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-60 disabled:cursor-not-allowed"
                >
                  <option value="">
                    {availableJumpConnections.length === 0
                      ? t('remoteConnection.jump.noConnections')
                      : t('remoteConnection.jump.selectConnection')}
                  </option>
                  {availableJumpConnections.map((connection) => (
                    <option key={connection.id} value={connection.id}>
                      {getConnectionLabel(connection)}
                    </option>
                  ))}
                </select>
              </div>
              {selectedJumpConnection ? (
                <div className="rounded border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
                  {t('remoteConnection.jump.referenceAuthHint', { auth: selectedJumpAuthLabel })}
                  {' '}
                  {selectedJumpConnection.username}@{selectedJumpConnection.host}:{selectedJumpConnection.port || 22}
                </div>
              ) : (
                <div className="text-xs text-muted-foreground">
                  {t('remoteConnection.jump.existingHint')}
                </div>
              )}
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-sm text-muted-foreground">{t('remoteConnection.jump.host')}</label>
                <input
                  value={remoteJumpHost}
                  onChange={(e) => onRemoteJumpHostChange(e.target.value)}
                  className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                  placeholder="bastion.example.com"
                />
              </div>
              <div>
                <label className="text-sm text-muted-foreground">{t('remoteConnection.jump.port')}</label>
                <input
                  value={remoteJumpPort}
                  onChange={(e) => onRemoteJumpPortChange(e.target.value)}
                  className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                  placeholder="22"
                />
              </div>
              <div>
                <label className="text-sm text-muted-foreground">{t('remoteConnection.jump.username')}</label>
                <input
                  value={remoteJumpUsername}
                  onChange={(e) => onRemoteJumpUsernameChange(e.target.value)}
                  className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                  placeholder="jump_user"
                />
              </div>
              <div>
                <label className="text-sm text-muted-foreground">{t('remoteConnection.jump.privateKeyPath')}</label>
                <div className="mt-1 flex items-center gap-2">
                  <input
                    value={remoteJumpPrivateKeyPath}
                    onChange={(e) => onRemoteJumpPrivateKeyPathChange(e.target.value)}
                    className="flex-1 px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="/Users/you/.ssh/jump_key"
                  />
                  <button
                    type="button"
                    onClick={() => onOpenKeyFilePicker('jump_private_key')}
                    className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
                  >
                    {t('remoteConnection.selectFile')}
                  </button>
                </div>
              </div>
              <div>
                <label className="text-sm text-muted-foreground">{t('remoteConnection.jump.certificatePath')}</label>
                <div className="mt-1 flex items-center gap-2">
                  <input
                    value={remoteJumpCertificatePath}
                    onChange={(e) => onRemoteJumpCertificatePathChange(e.target.value)}
                    className="flex-1 px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="/Users/you/.ssh/jump_key-cert.pub"
                  />
                  <button
                    type="button"
                    onClick={() => onOpenKeyFilePicker('jump_certificate')}
                    className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
                  >
                    {t('remoteConnection.selectFile')}
                  </button>
                </div>
              </div>
              <div className="col-span-2">
                <label className="text-sm text-muted-foreground">{t('remoteConnection.jump.password')}</label>
                <PasswordField
                  value={remoteJumpPassword}
                  onChange={onRemoteJumpPasswordChange}
                  placeholder={t('remoteConnection.jump.passwordPlaceholder')}
                  autoComplete="current-password"
                />
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
