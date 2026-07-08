// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import { useI18n } from '../../../i18n/I18nProvider';

interface RemoteConnectionModalFooterProps {
  editingRemoteConnection: boolean;
  remoteTesting: boolean;
  remoteSaving: boolean;
  onClose: () => void;
  onTest: () => void;
  onSave: () => void;
}

export const RemoteConnectionModalFooter: FC<RemoteConnectionModalFooterProps> = ({
  editingRemoteConnection,
  remoteTesting,
  remoteSaving,
  onClose,
  onTest,
  onSave,
}) => {
  const { t } = useI18n();

  return (
    <div className="mt-6 flex justify-end gap-2">
      <button
        onClick={onClose}
        className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
      >
        {t('common.cancel')}
      </button>
      <button
        onClick={onTest}
        disabled={remoteTesting || remoteSaving}
        className="px-4 py-2 rounded border border-border text-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {remoteTesting ? t('remoteConnection.testing') : t('remoteConnection.test')}
      </button>
      <button
        onClick={onSave}
        disabled={remoteSaving || remoteTesting}
        className="px-4 py-2 rounded bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {remoteSaving ? t('common.saving') : editingRemoteConnection ? t('common.save') : t('common.create')}
      </button>
    </div>
  );
};
