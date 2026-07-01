// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useEffect, useState } from 'react';
import { useI18n } from '../i18n/I18nProvider';
import { useApiClient } from '../lib/api/ApiClientContext';
import { useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { cn } from '../lib/utils';
import RemoteVerificationModal from './remote/RemoteVerificationModal';
import { LocalBrowserPane, RemoteBrowserPane } from './remoteSftp/SftpBrowsers';
import { TransferQueuePanel, TransferStatusBanner } from './remoteSftp/TransferPanels';
import { useDialogService } from './ui/DialogProvider';
import {
  formatBytes,
  joinLocalPath,
  joinRemotePath,
} from './remoteSftp/helpers';
import { useRemoteSftpBrowsers } from './remoteSftp/useRemoteSftpBrowsers';
import { useRemoteSftpRemoteActions } from './remoteSftp/useRemoteSftpRemoteActions';
import { useRemoteSftpTransfer } from './remoteSftp/useRemoteSftpTransfer';
import { useRemoteSftpVerification } from './remoteSftp/useRemoteSftpVerification';

interface RemoteSftpPanelProps {
  className?: string;
}

const RemoteSftpPanel: React.FC<RemoteSftpPanelProps> = ({ className }) => {
  const { t } = useI18n();
  const {
    currentRemoteConnection,
    selectRemoteConnection,
  } = useChatStoreFromContext();
  const client = useApiClient();
  const { confirm, prompt } = useDialogService();
  const currentRemoteConnectionId = currentRemoteConnection?.id ?? null;
  const currentRemoteDefaultPath = currentRemoteConnection?.defaultRemotePath || '.';

  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const {
    activeVerificationCode,
    verificationOpen,
    verificationPrompt,
    verificationCodeInput,
    verificationSubmitting,
    setVerificationCodeInput,
    getVerificationCode,
    handleSecondFactorRequired,
    resetVerificationState,
    closeVerification,
    submitVerification,
  } = useRemoteSftpVerification({
    setError,
    setMessage,
    t,
  });
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
    t,
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
    t,
  });
  const {
    remoteActionLoading,
    handleCreateRemoteDirectory,
    handleRenameRemoteEntry,
    handleDeleteRemoteEntry,
  } = useRemoteSftpRemoteActions({
    client,
    currentRemoteConnection,
    remotePath,
    selectedRemote,
    setSelectedRemote,
    loadRemote,
    activeVerificationCode,
    setMessage,
    setError,
    prompt,
    confirm,
    onSecondFactorRequired: handleSecondFactorRequired,
    t,
  });

  useEffect(() => {
    if (!currentRemoteConnectionId) return;
    resetTransferState();
    setMessage(null);
    setError(null);
    resetVerificationState();
  }, [currentRemoteConnectionId, resetTransferState, resetVerificationState]);

  const handleUpload = async () => {
    if (!currentRemoteConnection) return;
    if (!selectedLocal) {
      setError(t('remote.sftp.error.selectLocal'));
      return;
    }
    const target = joinRemotePath(remotePath, selectedLocal.name);
    enqueueTransfer({
      direction: 'upload',
      localSource: selectedLocal.path,
      remoteSource: target,
      fallbackSuccess: t('remote.sftp.success.upload', { name: selectedLocal.name }),
      label: `${t('remote.sftp.transfer.uploading')} ${selectedLocal.name}`,
    });
  };

  const handleDownload = async () => {
    if (!currentRemoteConnection) return;
    if (!selectedRemote) {
      setError(t('remote.sftp.error.selectRemote'));
      return;
    }
    if (!localPath) {
      setError(t('remote.sftp.error.enterLocalDir'));
      return;
    }
    const target = joinLocalPath(localPath, selectedRemote.name);
    enqueueTransfer({
      direction: 'download',
      localSource: target,
      remoteSource: selectedRemote.path,
      fallbackSuccess: t('remote.sftp.success.download', { name: selectedRemote.name }),
      label: `${t('remote.sftp.transfer.downloading')} ${selectedRemote.name}`,
    });
  };

  if (!currentRemoteConnection) {
    return (
      <div className={cn('flex h-full items-center justify-center text-muted-foreground', className)}>
        {t('remote.common.chooseConnection')}
      </div>
    );
  }

  return (
    <div className={cn('flex h-full flex-col bg-card', className)}>
      <div className="flex items-center justify-between border-b border-border px-4 py-2 gap-3">
        <div className="min-w-0">
          <div className="text-sm font-medium text-foreground truncate">{t('remote.sftp.title', { name: currentRemoteConnection.name })}</div>
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
            {t('remote.sftp.upload')}
          </button>
          <button
            type="button"
            onClick={handleDownload}
            disabled={remoteActionLoading}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {t('remote.sftp.download')}
          </button>
          <button
            type="button"
            onClick={() => void selectRemoteConnection(currentRemoteConnection.id)}
            className="rounded border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
          >
            {t('remote.sftp.backToTerminal')}
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
        onClose={closeVerification}
        onSubmit={() => {
          void submitVerification();
        }}
      />
    </div>
  );
};

export default RemoteSftpPanel;
