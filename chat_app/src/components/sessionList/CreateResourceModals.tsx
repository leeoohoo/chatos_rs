import React from 'react';

import ManagerFormDialog from '../ui/ManagerFormDialog';
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
  return (
    <ManagerFormDialog
      open={isOpen}
      title={title}
      widthClassName="max-w-xl"
      onClose={onClose}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
        className="space-y-4"
      >
        <div className="space-y-4 rounded-xl border border-border bg-muted/40 p-4">
          <div>
            <label className="text-sm text-muted-foreground">{pathLabel}</label>
            <div className="mt-1 flex items-center gap-2">
              <input
                value={pathValue}
                onChange={(e) => onPathChange(e.target.value)}
                className="flex-1 rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                placeholder="选择或输入本地目录路径"
                autoFocus
              />
              <button
                type="button"
                onClick={onOpenPicker}
                className="rounded-lg bg-muted px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent"
              >
                选择目录
              </button>
            </div>
          </div>
          {pathValue.trim() ? (
            <div className="text-xs text-muted-foreground">
              {title.includes('项目') ? '项目名称将默认使用：' : '终端名称将默认使用：'}
              <span className="text-foreground">{deriveNameFromPath(pathValue, fallbackName)}</span>
            </div>
          ) : null}
          {error ? (
            <div className="text-xs text-destructive">{error}</div>
          ) : null}
        </div>
        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg bg-muted px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent"
          >
            取消
          </button>
          <button
            type="submit"
            className="rounded-lg bg-primary px-4 py-2 text-sm text-primary-foreground transition-opacity hover:opacity-90"
          >
            创建
          </button>
        </div>
      </form>
    </ManagerFormDialog>
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
