// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import { useI18n } from '../../../i18n/I18nProvider';
import type { HostKeyPolicy } from '../helpers';

interface ConnectionBasicsSectionProps {
  remoteName: string;
  remoteHost: string;
  remotePort: string;
  remoteUsername: string;
  remoteHostKeyPolicy: HostKeyPolicy;
  onRemoteNameChange: (value: string) => void;
  onRemoteHostChange: (value: string) => void;
  onRemotePortChange: (value: string) => void;
  onRemoteUsernameChange: (value: string) => void;
  onRemoteHostKeyPolicyChange: (value: HostKeyPolicy) => void;
}

export const ConnectionBasicsSection: FC<ConnectionBasicsSectionProps> = ({
  remoteName,
  remoteHost,
  remotePort,
  remoteUsername,
  remoteHostKeyPolicy,
  onRemoteNameChange,
  onRemoteHostChange,
  onRemotePortChange,
  onRemoteUsernameChange,
  onRemoteHostKeyPolicyChange,
}) => {
  const { t } = useI18n();

  return (
    <div className="grid grid-cols-2 gap-3">
      <div className="col-span-2">
        <label className="text-sm text-muted-foreground">{t('remoteConnection.name')}</label>
        <input
          value={remoteName}
          onChange={(e) => onRemoteNameChange(e.target.value)}
          className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          placeholder={t('remoteConnection.namePlaceholder')}
        />
      </div>
      <div>
        <label className="text-sm text-muted-foreground">{t('remoteConnection.host')}</label>
        <input
          value={remoteHost}
          onChange={(e) => onRemoteHostChange(e.target.value)}
          className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          placeholder={t('remoteConnection.hostPlaceholder')}
        />
      </div>
      <div>
        <label className="text-sm text-muted-foreground">{t('remoteConnection.port')}</label>
        <input
          value={remotePort}
          onChange={(e) => onRemotePortChange(e.target.value)}
          className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          placeholder="22"
        />
      </div>
      <div>
        <label className="text-sm text-muted-foreground">{t('remoteConnection.username')}</label>
        <input
          value={remoteUsername}
          onChange={(e) => onRemoteUsernameChange(e.target.value)}
          className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          placeholder="root"
        />
      </div>
      <div>
        <label className="text-sm text-muted-foreground">{t('remoteConnection.hostKeyPolicy')}</label>
        <select
          value={remoteHostKeyPolicy}
          onChange={(e) => onRemoteHostKeyPolicyChange(e.target.value as HostKeyPolicy)}
          className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
        >
          <option value="strict">{t('remoteConnection.hostKeyPolicy.strict')}</option>
          <option value="accept_new">{t('remoteConnection.hostKeyPolicy.acceptNew')}</option>
        </select>
      </div>
    </div>
  );
};
