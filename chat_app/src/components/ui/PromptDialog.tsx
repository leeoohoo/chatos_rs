import React from 'react';

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
  inputLabel = '输入内容',
  placeholder,
  value,
  error,
  confirmText = '确认',
  cancelText = '取消',
  type = 'info',
  onValueChange,
  onConfirm,
  onCancel,
}) => {
  if (!isOpen) return null;

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
            {inputLabel}
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
              {cancelText}
            </button>
            <button
              type="button"
              onClick={onConfirm}
              className={`flex-1 rounded-md px-4 py-2 text-sm transition-colors ${getConfirmButtonStyle()}`}
            >
              {confirmText}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default PromptDialog;
