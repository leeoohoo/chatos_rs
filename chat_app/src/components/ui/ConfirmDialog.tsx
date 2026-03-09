import React from 'react';

interface ConfirmDialogProps {
  isOpen: boolean;
  title: string;
  message: string;
  description?: string;
  details?: string;
  detailsTitle?: string;
  detailsLines?: string[];
  confirmText?: string;
  cancelText?: string;
  onConfirm: () => void;
  onCancel: () => void;
  type?: 'danger' | 'warning' | 'info';
}

const ConfirmDialog: React.FC<ConfirmDialogProps> = ({
  isOpen,
  title,
  message,
  description,
  details,
  detailsTitle = '详情/建议操作',
  detailsLines,
  confirmText = '确认',
  cancelText = '取消',
  onConfirm,
  onCancel,
  type = 'danger'
}) => {
  if (!isOpen) return null;
  const normalizedDetailLines = (detailsLines || [])
    .map((line) => (line || '').trim())
    .filter((line) => line.length > 0);
  const detailBlocks = normalizedDetailLines.length > 0
    ? normalizedDetailLines
    : (details && details.trim().length > 0 ? [details.trim()] : []);

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
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div className="bg-card rounded-lg shadow-lg w-full max-w-sm border border-border animate-breathing-border">
        {/* 内容区域 */}
        <div className="p-6">
          <h3 className="text-lg font-medium text-foreground mb-2">
            {title}
          </h3>
          <p className="text-muted-foreground text-sm mb-3">
            {description || message}
          </p>
          {detailBlocks.length > 0 && (
            <div className="mb-6 rounded border border-border bg-muted/40 px-3 py-2 text-xs text-foreground/80 whitespace-pre-wrap">
              <div className="mb-1 text-[11px] font-semibold uppercase tracking-wide text-foreground/70">
                {detailsTitle}
              </div>
              <div className="space-y-1">
                {detailBlocks.map((line, index) => (
                  <div key={`${line}-${index}`}>{line}</div>
                ))}
              </div>
            </div>
          )}
          {detailBlocks.length === 0 && <div className="mb-6" />}

          {/* 按钮区域 */}
          <div className="flex gap-3">
            <button
              onClick={onCancel}
              className="flex-1 px-4 py-2 text-sm border border-border rounded-md hover:bg-accent transition-colors"
            >
              {cancelText}
            </button>
            <button
              onClick={onConfirm}
              className={`flex-1 px-4 py-2 text-sm rounded-md transition-colors ${getConfirmButtonStyle()}`}
            >
              {confirmText}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default ConfirmDialog;
