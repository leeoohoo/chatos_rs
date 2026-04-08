import React from 'react';

import { deriveNameFromPath } from './helpers';

interface CreateResourceModalProps {
  isOpen: boolean;
  title: string;
  pathLabel: string;
  pathValue: string;
  error: string | null;
  fallbackName: string;
  onClose: () => void;
  onPathChange: (value: string) => void;
  onOpenPicker: () => void;
  onSubmit: () => void;
}

const CreateResourceModal: React.FC<CreateResourceModalProps> = ({
  isOpen,
  title,
  pathLabel,
  pathValue,
  error,
  fallbackName,
  onClose,
  onPathChange,
  onOpenPicker,
  onSubmit,
}) => {
  if (!isOpen) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div className="relative bg-card border border-border rounded-lg shadow-xl w-[520px] p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold text-foreground">{title}</h3>
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
          <div>
            <label className="text-sm text-muted-foreground">{pathLabel}</label>
            <div className="mt-1 flex items-center gap-2">
              <input
                value={pathValue}
                onChange={(e) => onPathChange(e.target.value)}
                className="flex-1 px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                placeholder="选择或输入本地目录路径"
              />
              <button
                type="button"
                onClick={onOpenPicker}
                className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
              >
                选择目录
              </button>
            </div>
          </div>
          {pathValue.trim() && (
            <div className="text-xs text-muted-foreground">
              {title.includes('项目') ? '项目名称将默认使用：' : '终端名称将默认使用：'}
              <span className="text-foreground">{deriveNameFromPath(pathValue, fallbackName)}</span>
            </div>
          )}
          {error && (
            <div className="text-xs text-destructive">{error}</div>
          )}
        </div>
        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
          >
            取消
          </button>
          <button
            onClick={onSubmit}
            className="px-4 py-2 rounded bg-primary text-primary-foreground hover:bg-primary/90"
          >
            创建
          </button>
        </div>
      </div>
    </div>
  );
};

interface CreateProjectModalProps {
  isOpen: boolean;
  projectRoot: string;
  projectError: string | null;
  onClose: () => void;
  onProjectRootChange: (value: string) => void;
  onOpenPicker: () => void;
  onCreate: () => void;
}

export const CreateProjectModal: React.FC<CreateProjectModalProps> = ({
  isOpen,
  projectRoot,
  projectError,
  onClose,
  onProjectRootChange,
  onOpenPicker,
  onCreate,
}) => {
  return (
    <CreateResourceModal
      isOpen={isOpen}
      title="新增项目"
      pathLabel="项目目录"
      pathValue={projectRoot}
      error={projectError}
      fallbackName="Project"
      onClose={onClose}
      onPathChange={onProjectRootChange}
      onOpenPicker={onOpenPicker}
      onSubmit={onCreate}
    />
  );
};

interface CreateTerminalModalProps {
  isOpen: boolean;
  terminalRoot: string;
  terminalError: string | null;
  onClose: () => void;
  onTerminalRootChange: (value: string) => void;
  onOpenPicker: () => void;
  onCreate: () => void;
}

export const CreateTerminalModal: React.FC<CreateTerminalModalProps> = ({
  isOpen,
  terminalRoot,
  terminalError,
  onClose,
  onTerminalRootChange,
  onOpenPicker,
  onCreate,
}) => {
  return (
    <CreateResourceModal
      isOpen={isOpen}
      title="新增终端"
      pathLabel="终端目录"
      pathValue={terminalRoot}
      error={terminalError}
      fallbackName="Terminal"
      onClose={onClose}
      onPathChange={onTerminalRootChange}
      onOpenPicker={onOpenPicker}
      onSubmit={onCreate}
    />
  );
};
