// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { useI18n } from '../../i18n/I18nProvider';

interface RemoteVerificationModalProps {
  isOpen: boolean;
  prompt?: string | null;
  code: string;
  submitting?: boolean;
  onCodeChange: (value: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}

const RemoteVerificationModal: React.FC<RemoteVerificationModalProps> = ({
  isOpen,
  prompt,
  code,
  submitting = false,
  onCodeChange,
  onClose,
  onSubmit,
}) => {
  const { t } = useI18n();

  if (!isOpen) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-[70] flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={submitting ? undefined : onClose} />
      <div className="relative w-[520px] max-w-[92vw] rounded-xl border border-border bg-card p-6 shadow-2xl">
        <h3 className="text-xl font-semibold text-foreground">{t('remoteVerification.title')}</h3>
        <p className="mt-2 text-sm text-muted-foreground">
          {prompt?.trim() || t('remoteVerification.promptFallback')}
        </p>

        <input
          autoFocus
          type="text"
          value={code}
          onChange={(event) => onCodeChange(event.target.value)}
          className="mt-4 w-full rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          placeholder={t('remoteVerification.codePlaceholder')}
          disabled={submitting}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              onSubmit();
            }
          }}
        />

        <div className="mt-6 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            disabled={submitting}
            className="rounded bg-muted px-4 py-2 text-muted-foreground hover:bg-accent disabled:opacity-50"
          >
            {t('remoteVerification.close')}
          </button>
          <button
            type="button"
            onClick={onSubmit}
            disabled={submitting || !code.trim()}
            className="rounded bg-primary px-4 py-2 text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            {submitting ? t('remoteVerification.continuing') : t('remoteVerification.continue')}
          </button>
        </div>
      </div>
    </div>
  );
};

export default RemoteVerificationModal;
