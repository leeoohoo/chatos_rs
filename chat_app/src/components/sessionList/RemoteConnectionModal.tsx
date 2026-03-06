import React from 'react';

import type { HostKeyPolicy, KeyFilePickerTarget, RemoteAuthType } from './helpers';

interface RemoteConnectionModalProps {
  isOpen: boolean;
  editingRemoteConnection: boolean;
  remoteName: string;
  remoteHost: string;
  remotePort: string;
  remoteUsername: string;
  remoteAuthType: RemoteAuthType;
  remotePassword: string;
  remotePrivateKeyPath: string;
  remoteCertificatePath: string;
  remoteDefaultPath: string;
  remoteHostKeyPolicy: HostKeyPolicy;
  remoteJumpEnabled: boolean;
  remoteJumpHost: string;
  remoteJumpPort: string;
  remoteJumpUsername: string;
  remoteJumpPrivateKeyPath: string;
  remoteJumpPassword: string;
  remoteError: string | null;
  remoteSuccess: string | null;
  remoteTesting: boolean;
  remoteSaving: boolean;
  onClose: () => void;
  onRemoteNameChange: (value: string) => void;
  onRemoteHostChange: (value: string) => void;
  onRemotePortChange: (value: string) => void;
  onRemoteUsernameChange: (value: string) => void;
  onRemoteAuthTypeChange: (value: RemoteAuthType) => void;
  onRemotePasswordChange: (value: string) => void;
  onRemotePrivateKeyPathChange: (value: string) => void;
  onRemoteCertificatePathChange: (value: string) => void;
  onRemoteDefaultPathChange: (value: string) => void;
  onRemoteHostKeyPolicyChange: (value: HostKeyPolicy) => void;
  onRemoteJumpEnabledChange: (value: boolean) => void;
  onRemoteJumpHostChange: (value: string) => void;
  onRemoteJumpPortChange: (value: string) => void;
  onRemoteJumpUsernameChange: (value: string) => void;
  onRemoteJumpPrivateKeyPathChange: (value: string) => void;
  onRemoteJumpPasswordChange: (value: string) => void;
  onOpenKeyFilePicker: (target: KeyFilePickerTarget) => void;
  onTest: () => void;
  onSave: () => void;
}

export const RemoteConnectionModal: React.FC<RemoteConnectionModalProps> = ({
  isOpen,
  editingRemoteConnection,
  remoteName,
  remoteHost,
  remotePort,
  remoteUsername,
  remoteAuthType,
  remotePassword,
  remotePrivateKeyPath,
  remoteCertificatePath,
  remoteDefaultPath,
  remoteHostKeyPolicy,
  remoteJumpEnabled,
  remoteJumpHost,
  remoteJumpPort,
  remoteJumpUsername,
  remoteJumpPrivateKeyPath,
  remoteJumpPassword,
  remoteError,
  remoteSuccess,
  remoteTesting,
  remoteSaving,
  onClose,
  onRemoteNameChange,
  onRemoteHostChange,
  onRemotePortChange,
  onRemoteUsernameChange,
  onRemoteAuthTypeChange,
  onRemotePasswordChange,
  onRemotePrivateKeyPathChange,
  onRemoteCertificatePathChange,
  onRemoteDefaultPathChange,
  onRemoteHostKeyPolicyChange,
  onRemoteJumpEnabledChange,
  onRemoteJumpHostChange,
  onRemoteJumpPortChange,
  onRemoteJumpUsernameChange,
  onRemoteJumpPrivateKeyPathChange,
  onRemoteJumpPasswordChange,
  onOpenKeyFilePicker,
  onTest,
  onSave,
}) => {
  if (!isOpen) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div className="relative bg-card border border-border rounded-lg shadow-xl w-[620px] p-6 max-h-[85vh] overflow-y-auto">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold text-foreground">
            {editingRemoteConnection ? '编辑远端连接' : '新增远端连接'}
          </h3>
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
          <div className="grid grid-cols-2 gap-3">
            <div className="col-span-2">
              <label className="text-sm text-muted-foreground">名称（可选）</label>
              <input
                value={remoteName}
                onChange={(e) => onRemoteNameChange(e.target.value)}
                className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                placeholder="默认：user@host"
              />
            </div>
            <div>
              <label className="text-sm text-muted-foreground">主机</label>
              <input
                value={remoteHost}
                onChange={(e) => onRemoteHostChange(e.target.value)}
                className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                placeholder="例如 1.2.3.4"
              />
            </div>
            <div>
              <label className="text-sm text-muted-foreground">端口</label>
              <input
                value={remotePort}
                onChange={(e) => onRemotePortChange(e.target.value)}
                className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                placeholder="22"
              />
            </div>
            <div>
              <label className="text-sm text-muted-foreground">用户名</label>
              <input
                value={remoteUsername}
                onChange={(e) => onRemoteUsernameChange(e.target.value)}
                className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                placeholder="root"
              />
            </div>
            <div>
              <label className="text-sm text-muted-foreground">主机校验策略</label>
              <select
                value={remoteHostKeyPolicy}
                onChange={(e) => onRemoteHostKeyPolicyChange(e.target.value as HostKeyPolicy)}
                className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              >
                <option value="strict">strict</option>
                <option value="accept_new">accept_new</option>
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="text-sm text-muted-foreground">认证方式</label>
              <select
                value={remoteAuthType}
                onChange={(e) => onRemoteAuthTypeChange(e.target.value as RemoteAuthType)}
                className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              >
                <option value="private_key">private_key</option>
                <option value="private_key_cert">private_key_cert</option>
                <option value="password">password</option>
              </select>
            </div>
            <div>
              <label className="text-sm text-muted-foreground">默认远端目录（可选）</label>
              <input
                value={remoteDefaultPath}
                onChange={(e) => onRemoteDefaultPathChange(e.target.value)}
                className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                placeholder="例如 /home/root"
              />
            </div>
            {remoteAuthType === 'password' ? (
              <div className="col-span-2">
                <label className="text-sm text-muted-foreground">密码</label>
                <input
                  type="password"
                  value={remotePassword}
                  onChange={(e) => onRemotePasswordChange(e.target.value)}
                  className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                  placeholder="请输入 SSH 登录密码"
                />
              </div>
            ) : (
              <>
                <div className="col-span-2">
                  <label className="text-sm text-muted-foreground">私钥路径</label>
                  <div className="mt-1 flex items-center gap-2">
                    <input
                      value={remotePrivateKeyPath}
                      onChange={(e) => onRemotePrivateKeyPathChange(e.target.value)}
                      className="flex-1 px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                      placeholder="/Users/you/.ssh/id_rsa"
                    />
                    <button
                      type="button"
                      onClick={() => onOpenKeyFilePicker('private_key')}
                      className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
                    >
                      选择文件
                    </button>
                  </div>
                </div>
                {remoteAuthType === 'private_key_cert' && (
                  <div className="col-span-2">
                    <label className="text-sm text-muted-foreground">证书路径</label>
                    <div className="mt-1 flex items-center gap-2">
                      <input
                        value={remoteCertificatePath}
                        onChange={(e) => onRemoteCertificatePathChange(e.target.value)}
                        className="flex-1 px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                        placeholder="/Users/you/.ssh/id_rsa-cert.pub"
                      />
                      <button
                        type="button"
                        onClick={() => onOpenKeyFilePicker('certificate')}
                        className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
                      >
                        选择文件
                      </button>
                    </div>
                  </div>
                )}
              </>
            )}
          </div>

          <div className="rounded border border-border p-3 space-y-3">
            <label className="inline-flex items-center gap-2 text-sm text-foreground">
              <input
                type="checkbox"
                checked={remoteJumpEnabled}
                onChange={(e) => onRemoteJumpEnabledChange(e.target.checked)}
              />
              启用跳板机
            </label>

            {remoteJumpEnabled && (
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="text-sm text-muted-foreground">跳板机主机</label>
                  <input
                    value={remoteJumpHost}
                    onChange={(e) => onRemoteJumpHostChange(e.target.value)}
                    className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="bastion.example.com"
                  />
                </div>
                <div>
                  <label className="text-sm text-muted-foreground">跳板机端口</label>
                  <input
                    value={remoteJumpPort}
                    onChange={(e) => onRemoteJumpPortChange(e.target.value)}
                    className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="22"
                  />
                </div>
                <div>
                  <label className="text-sm text-muted-foreground">跳板机用户名</label>
                  <input
                    value={remoteJumpUsername}
                    onChange={(e) => onRemoteJumpUsernameChange(e.target.value)}
                    className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="jump_user"
                  />
                </div>
                <div>
                  <label className="text-sm text-muted-foreground">跳板机私钥路径（可选）</label>
                  <div className="mt-1 flex items-center gap-2">
                    <input
                      value={remoteJumpPrivateKeyPath}
                      onChange={(e) => onRemoteJumpPrivateKeyPathChange(e.target.value)}
                      className="flex-1 px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                      placeholder="/Users/you/.ssh/jump_key"
                    />
                    <button
                      type="button"
                      onClick={() => onOpenKeyFilePicker('jump_private_key')}
                      className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
                    >
                      选择文件
                    </button>
                  </div>
                </div>
                <div className="col-span-2">
                  <label className="text-sm text-muted-foreground">跳板机密码（可选）</label>
                  <input
                    type="password"
                    value={remoteJumpPassword}
                    onChange={(e) => onRemoteJumpPasswordChange(e.target.value)}
                    className="mt-1 w-full px-3 py-2 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="留空则尝试私钥/Agent/目标密码"
                  />
                </div>
              </div>
            )}
          </div>

          {remoteError && <div className="text-xs text-destructive">{remoteError}</div>}
          {remoteSuccess && <div className="text-xs text-emerald-600">{remoteSuccess}</div>}
        </div>
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
      </div>
    </div>
  );
};
