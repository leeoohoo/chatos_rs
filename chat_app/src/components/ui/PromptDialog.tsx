// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { useI18n } from '../../i18n/I18nProvider';

interface PromptDialogProps {
  isOpen: boolean;
  title: string;
  message: string;
  description?: string;
  inputLabel?: string;
  placeholder?: string;
  value: string;
  error?: string | null;
  confirmText?: string;
  cancelText?: string;
  type?: 'danger' | 'warning' | 'info';
  onValueChange: (value: string) => void;
  onConfirm: () => void;
  onCancel: () => void;
}

const PromptDialog: React.FC<PromptDialogProps> = ({
  isOpen,
  title,
  message,
  description,
  inputLabel,
  placeholder,
  value,
  error,
  confirmText,
  cancelText,
  type = 'info',
  onValueChange,
  onConfirm,
  onCancel,
}) => {
  const { t } = useI18n();
  if (!isOpen) return null;
  const effectiveInputLabel = inputLabel || t('dialog.inputLabel');
  const effectiveConfirmText = confirmText || t('common.confirm');
  const effectiveCancelText = cancelText || t('common.cancel');

  const getConfirmButtonStyle = () => {
    switch (type) {
      case 'danger':
        return 'bg-red-600 hover:bg-red-700 text-white';
      case 'warning':
        return 'bg-yellow-600 hover:bg-yellow-700 text-white';
      default:
        return 'bg-blue-600 hover:bg-blue-700 text-white';
    }
  };

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50 p-4">
      <div className="w-full max-w-md rounded-lg border border-border bg-card shadow-lg">
        <div className="p-6">
          <h3 className="mb-2 text-lg font-medium text-foreground">
            {title}
          </h3>
          <p className="mb-4 text-sm text-muted-foreground">
            {description || message}
          </p>

          <label className="mb-2 block text-xs font-medium uppercase tracking-wide text-foreground/70">
            {effectiveInputLabel}
          </label>
          <input
            autoFocus
            type="text"
            value={value}
            placeholder={placeholder}
            onChange={(event) => onValueChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                onConfirm();
              }
            }}
            className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition-colors focus:border-primary"
          />
          {error ? (
            <div className="mt-2 text-xs text-destructive">
              {error}
            </div>
          ) : (
            <div className="mt-2 h-4" />
          )}

          <div className="mt-6 flex gap-3">
            <button
              type="button"
              onClick={onCancel}
              className="flex-1 rounded-md border border-border px-4 py-2 text-sm transition-colors hover:bg-accent"
            >
              {effectiveCancelText}
            </button>
            <button
              type="button"
              onClick={onConfirm}
              className={`flex-1 rounded-md px-4 py-2 text-sm transition-colors ${getConfirmButtonStyle()}`}
            >
              {effectiveConfirmText}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default PromptDialog;
