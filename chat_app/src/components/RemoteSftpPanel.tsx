import React, { useEffect, useMemo, useState } from 'react';
import { useChatApiClientFromContext, useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { apiClient as globalApiClient } from '../lib/api/client';
import { resolveRemoteSftpErrorMessage } from '../lib/api/remoteConnectionErrors';
import { cn } from '../lib/utils';
import { LocalBrowserPane, RemoteBrowserPane } from './remoteSftp/SftpBrowsers';
import { TransferQueuePanel, TransferStatusBanner } from './remoteSftp/TransferPanels';
import {
  formatBytes,
  joinLocalPath,
  joinRemotePath,
  remoteDirname,
} from './remoteSftp/helpers';
import { useRemoteSftpBrowsers } from './remoteSftp/useRemoteSftpBrowsers';
import { useRemoteSftpTransfer } from './remoteSftp/useRemoteSftpTransfer';

interface RemoteSftpPanelProps {
  className?: string;
}

const RemoteSftpPanel: React.FC<RemoteSftpPanelProps> = ({ className }) => {
  const {
    currentRemoteConnection,
    selectRemoteConnection,
  } = useChatStoreFromContext();
  const apiClientFromContext = useChatApiClientFromContext();
  const client = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);
  const currentRemoteConnectionId = currentRemoteConnection?.id ?? null;
  const currentRemoteDefaultPath = currentRemoteConnection?.defaultRemotePath || '.';

  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [remoteActionLoading, setRemoteActionLoading] = useState(false);
  const {
    localPath,
    localParent,
    localEntries,
    localRoots,
    loadingLocal,
    selectedLocal,
    setSelectedLocal,
    remotePath,
    remoteParent,
    remoteEntries,
    loadingRemote,
    selectedRemote,
    setSelectedRemote,
    loadLocal,
    loadRemote,
    remotePathRef,
    localPathRef,
  } = useRemoteSftpBrowsers({
    client,
    currentRemoteConnectionId,
    currentRemoteDefaultPath,
    setError,
  });
  const {
    transfering,
    transferStatus,
    queuedTransfers,
    enqueueTransfer,
    handleRemoveQueuedTransfer,
    handleClearQueuedTransfers,
    handleCancelTransfer,
    resetTransferState,
  } = useRemoteSftpTransfer({
    client,
    currentRemoteConnectionId,
    loadLocal,
    loadRemote,
    remotePathRef,
    localPathRef,
    setMessage,
    setError,
  });

  useEffect(() => {
    if (!currentRemoteConnectionId) return;
    resetTransferState();
    setMessage(null);
    setError(null);
  }, [currentRemoteConnectionId, resetTransferState]);

  const handleUpload = async () => {
    if (!currentRemoteConnection) return;
    if (!selectedLocal) {
      setError('请先在右侧选择一个本地文件或目录');
      return;
    }
    const target = joinRemotePath(remotePath, selectedLocal.name);
    enqueueTransfer({
      direction: 'upload',
      localSource: selectedLocal.path,
      remoteSource: target,
      fallbackSuccess: `上传成功: ${selectedLocal.name}`,
      label: `上传 ${selectedLocal.name}`,
    });
  };

  const handleDownload = async () => {
    if (!currentRemoteConnection) return;
    if (!selectedRemote) {
      setError('请先在左侧选择一个远端文件或目录');
      return;
    }
    if (!localPath) {
      setError('请先在右侧进入一个本地目录');
      return;
    }
    const target = joinLocalPath(localPath, selectedRemote.name);
    enqueueTransfer({
      direction: 'download',
      localSource: target,
      remoteSource: selectedRemote.path,
      fallbackSuccess: `下载成功: ${selectedRemote.name}`,
      label: `下载 ${selectedRemote.name}`,
    });
  };

  const handleCreateRemoteDirectory = async () => {
    if (!currentRemoteConnection) return;
    const name = window.prompt('请输入新目录名称');
    if (name === null) return;
    const trimmedName = name.trim();
    if (!trimmedName) {
      setError('目录名称不能为空');
      return;
    }
    if (trimmedName === '.' || trimmedName === '..' || /[\\/]/.test(trimmedName)) {
      setError('目录名称不合法');
      return;
    }

    setRemoteActionLoading(true);
    setError(null);
    setMessage(null);
    try {
      await client.createRemoteSftpDirectory(currentRemoteConnection.id, remotePath, trimmedName);
      setMessage(`已创建目录: ${trimmedName}`);
      await loadRemote(remotePath);
    } catch (error) {
      setError(resolveRemoteSftpErrorMessage(error, '创建目录失败'));
    } finally {
      setRemoteActionLoading(false);
    }
  };

  const handleRenameRemoteEntry = async () => {
    if (!currentRemoteConnection) return;
    if (!selectedRemote) {
      setError('请先选择远端文件或目录');
      return;
    }
    const nextName = window.prompt('请输入新名称', selectedRemote.name);
    if (nextName === null) return;
    const trimmedName = nextName.trim();
    if (!trimmedName) {
      setError('新名称不能为空');
      return;
    }
    if (trimmedName === '.' || trimmedName === '..' || /[\\/]/.test(trimmedName)) {
      setError('新名称不合法');
      return;
    }
    if (trimmedName === selectedRemote.name) {
      return;
    }

    const targetPath = joinRemotePath(remoteDirname(selectedRemote.path), trimmedName);
    setRemoteActionLoading(true);
    setError(null);
    setMessage(null);
    try {
      await client.renameRemoteSftpEntry(currentRemoteConnection.id, selectedRemote.path, targetPath);
      setMessage(`已重命名: ${selectedRemote.name} → ${trimmedName}`);
      setSelectedRemote(null);
      await loadRemote(remotePath);
    } catch (error) {
      setError(resolveRemoteSftpErrorMessage(error, '重命名失败'));
    } finally {
      setRemoteActionLoading(false);
    }
  };

  const handleDeleteRemoteEntry = async () => {
    if (!currentRemoteConnection) return;
    if (!selectedRemote) {
      setError('请先选择远端文件或目录');
      return;
    }

    const confirmed = window.confirm(`确认删除 ${selectedRemote.isDir ? '目录' : '文件'} "${selectedRemote.name}" 吗？`);
    if (!confirmed) return;

    let recursive = false;
    if (selectedRemote.isDir) {
      recursive = window.confirm('是否递归删除该目录及其全部内容？\n选择“取消”将仅删除空目录。');
    }

    setRemoteActionLoading(true);
    setError(null);
    setMessage(null);
    try {
      await client.deleteRemoteSftpEntry(currentRemoteConnection.id, selectedRemote.path, recursive);
      setMessage(`已删除: ${selectedRemote.name}`);
      setSelectedRemote(null);
      await loadRemote(remotePath);
    } catch (error) {
      setError(resolveRemoteSftpErrorMessage(error, '删除失败'));
    } finally {
      setRemoteActionLoading(false);
    }
  };

  if (!currentRemoteConnection) {
    return (
      <div className={cn('flex h-full items-center justify-center text-muted-foreground', className)}>
        请选择一个远端连接
      </div>
    );
  }

  return (
    <div className={cn('flex h-full flex-col bg-card', className)}>
      <div className="flex items-center justify-between border-b border-border px-4 py-2 gap-3">
        <div className="min-w-0">
          <div className="text-sm font-medium text-foreground truncate">SFTP · {currentRemoteConnection.name}</div>
          <div className="text-xs text-muted-foreground truncate">
            {currentRemoteConnection.username}@{currentRemoteConnection.host}:{currentRemoteConnection.port}
          </div>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <button
            type="button"
            onClick={handleUpload}
            disabled={remoteActionLoading}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            上传 →
          </button>
          <button
            type="button"
            onClick={handleDownload}
            disabled={remoteActionLoading}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            ← 下载
          </button>
          <button
            type="button"
            onClick={() => void selectRemoteConnection(currentRemoteConnection.id)}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
          >
            返回终端
          </button>
        </div>
      </div>

      {message && <div className="px-4 py-2 text-xs text-emerald-600">{message}</div>}
      {error && <div className="px-4 py-2 text-xs text-destructive">{error}</div>}
      <TransferStatusBanner
        transferStatus={transferStatus}
        transfering={transfering}
        formatBytes={formatBytes}
        onCancelTransfer={() => {
          void handleCancelTransfer();
        }}
      />
      <TransferQueuePanel
        queuedTransfers={queuedTransfers}
        onClearQueuedTransfers={handleClearQueuedTransfers}
        onRemoveQueuedTransfer={handleRemoveQueuedTransfer}
      />

      <div className="flex flex-1 min-h-0 divide-x divide-border">
        <RemoteBrowserPane
          remotePath={remotePath}
          remoteParent={remoteParent}
          loadingRemote={loadingRemote}
          remoteEntries={remoteEntries}
          selectedRemote={selectedRemote}
          transfering={transfering}
          remoteActionLoading={remoteActionLoading}
          onCreateRemoteDirectory={() => {
            void handleCreateRemoteDirectory();
          }}
          onRenameRemoteEntry={() => {
            void handleRenameRemoteEntry();
          }}
          onDeleteRemoteEntry={() => {
            void handleDeleteRemoteEntry();
          }}
          onLoadRemoteParent={() => {
            void loadRemote(remoteParent || undefined);
          }}
          onRefreshRemote={() => {
            void loadRemote(remotePath);
          }}
          onSelectRemote={setSelectedRemote}
          onEnterRemoteDirectory={(entry) => {
            setSelectedRemote(null);
            void loadRemote(entry.path);
          }}
        />

        <LocalBrowserPane
          localPath={localPath}
          localParent={localParent}
          loadingLocal={loadingLocal}
          localRoots={localRoots}
          localEntries={localEntries}
          selectedLocal={selectedLocal}
          onLoadLocalParent={() => {
            void loadLocal(localParent);
          }}
          onRefreshLocal={() => {
            void loadLocal(localPath);
          }}
          onOpenLocalRoot={(entry) => {
            void loadLocal(entry.path);
          }}
          onSelectLocal={setSelectedLocal}
          onEnterLocalDirectory={(entry) => {
            setSelectedLocal(null);
            void loadLocal(entry.path);
          }}
        />
      </div>
    </div>
  );
};

export default RemoteSftpPanel;
