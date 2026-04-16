import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useChatApiClientFromContext, useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { apiClient as globalApiClient } from '../lib/api/client';
import { resolveRemoteSftpErrorMessage } from '../lib/api/remoteConnectionErrors';
import { cn } from '../lib/utils';
import RemoteVerificationModal from './remote/RemoteVerificationModal';
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
  const [activeVerificationCode, setActiveVerificationCode] = useState<string | null>(null);
  const [verificationOpen, setVerificationOpen] = useState(false);
  const [verificationPrompt, setVerificationPrompt] = useState('');
  const [verificationCodeInput, setVerificationCodeInput] = useState('');
  const [verificationSubmitting, setVerificationSubmitting] = useState(false);
  const pendingVerificationActionRef = useRef<((code: string) => Promise<void>) | null>(null);

  const isSecondFactorRequired = useCallback((err: unknown) => (
    typeof (err as any)?.code === 'string' && (err as any).code === 'second_factor_required'
  ), []);

  const extractSecondFactorPrompt = useCallback((err: unknown) => {
    const prompt = (err as any)?.payload?.challenge_prompt;
    if (typeof prompt === 'string' && prompt.trim()) {
      return prompt.trim();
    }
    return '请输入短信验证码或 OTP';
  }, []);

  const handleSecondFactorRequired = useCallback((
    err: unknown,
    retryWithCode: (code: string) => Promise<void>,
  ) => {
    if (!isSecondFactorRequired(err)) {
      return false;
    }
    pendingVerificationActionRef.current = retryWithCode;
    setVerificationPrompt(extractSecondFactorPrompt(err));
    setVerificationCodeInput('');
    setVerificationOpen(true);
    setVerificationSubmitting(false);
    setActiveVerificationCode(null);
    setError(null);
    setMessage(null);
    return true;
  }, [extractSecondFactorPrompt, isSecondFactorRequired]);

  const getVerificationCode = useCallback(() => activeVerificationCode, [activeVerificationCode]);
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
    getVerificationCode,
    onSecondFactorRequired: handleSecondFactorRequired,
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
    getVerificationCode,
    onSecondFactorRequired: handleSecondFactorRequired,
  });

  useEffect(() => {
    if (!currentRemoteConnectionId) return;
    resetTransferState();
    setMessage(null);
    setError(null);
    setActiveVerificationCode(null);
    setVerificationOpen(false);
    setVerificationPrompt('');
    setVerificationCodeInput('');
    setVerificationSubmitting(false);
    pendingVerificationActionRef.current = null;
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
      await client.createRemoteSftpDirectory(
        currentRemoteConnection.id,
        remotePath,
        trimmedName,
        activeVerificationCode || undefined,
      );
      setMessage(`已创建目录: ${trimmedName}`);
      await loadRemote(remotePath);
    } catch (err) {
      if (handleSecondFactorRequired(err, async (code) => {
        await client.createRemoteSftpDirectory(
          currentRemoteConnection.id,
          remotePath,
          trimmedName,
          code,
        );
        setMessage(`已创建目录: ${trimmedName}`);
        await loadRemote(remotePath, code);
      })) {
        return;
      }
      setError(resolveRemoteSftpErrorMessage(err, '创建目录失败'));
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
      await client.renameRemoteSftpEntry(
        currentRemoteConnection.id,
        selectedRemote.path,
        targetPath,
        activeVerificationCode || undefined,
      );
      setMessage(`已重命名: ${selectedRemote.name} → ${trimmedName}`);
      setSelectedRemote(null);
      await loadRemote(remotePath);
    } catch (err) {
      if (handleSecondFactorRequired(err, async (code) => {
        await client.renameRemoteSftpEntry(
          currentRemoteConnection.id,
          selectedRemote.path,
          targetPath,
          code,
        );
        setMessage(`已重命名: ${selectedRemote.name} → ${trimmedName}`);
        setSelectedRemote(null);
        await loadRemote(remotePath, code);
      })) {
        return;
      }
      setError(resolveRemoteSftpErrorMessage(err, '重命名失败'));
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
      await client.deleteRemoteSftpEntry(
        currentRemoteConnection.id,
        selectedRemote.path,
        recursive,
        activeVerificationCode || undefined,
      );
      setMessage(`已删除: ${selectedRemote.name}`);
      setSelectedRemote(null);
      await loadRemote(remotePath);
    } catch (err) {
      if (handleSecondFactorRequired(err, async (code) => {
        await client.deleteRemoteSftpEntry(
          currentRemoteConnection.id,
          selectedRemote.path,
          recursive,
          code,
        );
        setMessage(`已删除: ${selectedRemote.name}`);
        setSelectedRemote(null);
        await loadRemote(remotePath, code);
      })) {
        return;
      }
      setError(resolveRemoteSftpErrorMessage(err, '删除失败'));
    } finally {
      setRemoteActionLoading(false);
    }
  };

  const handleSubmitRemoteVerification = async () => {
    const code = verificationCodeInput.trim();
    if (!code) {
      setError('请输入验证码');
      return;
    }
    const pendingAction = pendingVerificationActionRef.current;
    if (!pendingAction) {
      setVerificationOpen(false);
      setError('验证码上下文已失效，请重试当前操作');
      return;
    }

    setVerificationSubmitting(true);
    setError(null);
    setMessage(null);
    try {
      await pendingAction(code);
      setActiveVerificationCode(code);
      setVerificationOpen(false);
      setVerificationPrompt('');
      setVerificationCodeInput('');
      pendingVerificationActionRef.current = null;
    } catch (err) {
      if (isSecondFactorRequired(err)) {
        setActiveVerificationCode(null);
        setVerificationPrompt(extractSecondFactorPrompt(err));
        setVerificationCodeInput('');
        setVerificationOpen(true);
        setError('验证码错误或已过期，请重试');
        return;
      }
      setVerificationOpen(false);
      setVerificationPrompt('');
      setVerificationCodeInput('');
      pendingVerificationActionRef.current = null;
      setError(resolveRemoteSftpErrorMessage(err, 'SFTP 操作失败'));
    } finally {
      setVerificationSubmitting(false);
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

      <RemoteVerificationModal
        isOpen={verificationOpen}
        prompt={verificationPrompt}
        code={verificationCodeInput}
        submitting={verificationSubmitting}
        onCodeChange={setVerificationCodeInput}
        onClose={() => {
          if (verificationSubmitting) {
            return;
          }
          setVerificationOpen(false);
          setVerificationPrompt('');
          setVerificationCodeInput('');
          pendingVerificationActionRef.current = null;
        }}
        onSubmit={() => {
          void handleSubmitRemoteVerification();
        }}
      />
    </div>
  );
};

export default RemoteSftpPanel;
