// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { useI18n } from '../../i18n/I18nProvider';

interface AlertDialogProps {
  isOpen: boolean;
  title: string;
  message: string;
  description?: string;
  confirmText?: string;
  type?: 'danger' | 'warning' | 'info';
  onConfirm: () => void;
}

const AlertDialog: React.FC<AlertDialogProps> = ({
  isOpen,
  title,
  message,
  description,
  confirmText,
  type = 'info',
  onConfirm,
}) => {
  const { t } = useI18n();
  if (!isOpen) return null;
  const effectiveConfirmText = confirmText || t('common.gotIt');

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
      <div className="w-full max-w-sm rounded-lg border border-border bg-card shadow-lg">
        <div className="p-6">
          <h3 className="mb-2 text-lg font-medium text-foreground">
            {title}
          </h3>
          <p className="mb-6 text-sm text-muted-foreground">
            {description || message}
          </p>

          <div className="flex justify-end">
            <button
              type="button"
              onClick={onConfirm}
              className={`rounded-md px-4 py-2 text-sm transition-colors ${getConfirmButtonStyle()}`}
            >
              {effectiveConfirmText}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default AlertDialog;
