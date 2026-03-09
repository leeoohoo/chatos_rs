import { useState, useCallback } from 'react';

interface ConfirmDialogState {
  isOpen: boolean;
  title: string;
  message: string;
  description?: string;
  details?: string;
  detailsTitle?: string;
  detailsLines?: string[];
  confirmText?: string;
  cancelText?: string;
  type?: 'danger' | 'warning' | 'info';
  onConfirm?: () => void;
  onCancel?: () => void;
}

export const useConfirmDialog = () => {
  const [dialogState, setDialogState] = useState<ConfirmDialogState>({
    isOpen: false,
    title: '',
    message: '',
    confirmText: '确认',
    cancelText: '取消',
    type: 'danger'
  });

  const showConfirmDialog = useCallback((options: {
    title: string;
    message: string;
    description?: string;
    details?: string;
    detailsTitle?: string;
    detailsLines?: string[];
    confirmText?: string;
    cancelText?: string;
    type?: 'danger' | 'warning' | 'info';
    onConfirm?: () => void;
    onCancel?: () => void;
  }) => {
    setDialogState({
      isOpen: true,
      title: options.title,
      message: options.message,
      description: options.description,
      details: options.details,
      detailsTitle: options.detailsTitle,
      detailsLines: options.detailsLines,
      confirmText: options.confirmText || '确认',
      cancelText: options.cancelText || '取消',
      type: options.type || 'danger',
      onConfirm: options.onConfirm,
      onCancel: options.onCancel
    });
  }, []);

  const hideConfirmDialog = useCallback(() => {
    setDialogState(prev => ({ ...prev, isOpen: false }));
  }, []);

  const handleConfirm = useCallback(() => {
    dialogState.onConfirm?.();
    hideConfirmDialog();
  }, [dialogState.onConfirm, hideConfirmDialog]);

  const handleCancel = useCallback(() => {
    dialogState.onCancel?.();
    hideConfirmDialog();
  }, [dialogState.onCancel, hideConfirmDialog]);

  return {
    dialogState,
    showConfirmDialog,
    hideConfirmDialog,
    handleConfirm,
    handleCancel
  };
};
