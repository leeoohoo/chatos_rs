import type { FC } from 'react';

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
}) => (
  <div className="mt-6 flex justify-end gap-2">
    <button
      onClick={onClose}
      className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
    >
      取消
    </button>
    <button
      onClick={onTest}
      disabled={remoteTesting || remoteSaving}
      className="px-4 py-2 rounded border border-border text-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
    >
      {remoteTesting ? '测试中...' : '测试连接'}
    </button>
    <button
      onClick={onSave}
      disabled={remoteSaving || remoteTesting}
      className="px-4 py-2 rounded bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
    >
      {remoteSaving ? '保存中...' : editingRemoteConnection ? '保存' : '创建'}
    </button>
  </div>
);
