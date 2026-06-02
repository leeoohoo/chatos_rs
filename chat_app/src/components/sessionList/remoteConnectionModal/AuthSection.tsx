import type { FC } from 'react';
import type {
  KeyFilePickerTarget,
  RemoteAuthType,
} from '../helpers';
import { PasswordField } from './PasswordField';

interface AuthSectionProps {
  remoteAuthType: RemoteAuthType;
  remotePassword: string;
  remotePrivateKeyPath: string;
  remoteCertificatePath: string;
  remoteDefaultPath: string;
  onRemoteAuthTypeChange: (value: RemoteAuthType) => void;
  onRemotePasswordChange: (value: string) => void;
  onRemotePrivateKeyPathChange: (value: string) => void;
  onRemoteCertificatePathChange: (value: string) => void;
  onRemoteDefaultPathChange: (value: string) => void;
  onOpenKeyFilePicker: (target: KeyFilePickerTarget) => void;
}

export const AuthSection: FC<AuthSectionProps> = ({
  remoteAuthType,
  remotePassword,
  remotePrivateKeyPath,
  remoteCertificatePath,
  remoteDefaultPath,
  onRemoteAuthTypeChange,
  onRemotePasswordChange,
  onRemotePrivateKeyPathChange,
  onRemoteCertificatePathChange,
  onRemoteDefaultPathChange,
  onOpenKeyFilePicker,
}) => (
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
        <PasswordField
          value={remotePassword}
          onChange={onRemotePasswordChange}
          placeholder="请输入 SSH 登录密码"
          autoComplete="current-password"
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
);
