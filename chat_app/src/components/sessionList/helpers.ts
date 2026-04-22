import type { FsEntry, RemoteConnection, Session } from '../../types';

export type RemoteAuthType = 'private_key' | 'private_key_cert' | 'password';
export type HostKeyPolicy = 'strict' | 'accept_new';
export type JumpHostMode = 'existing' | 'manual';
export type KeyFilePickerTarget =
  | 'private_key'
  | 'certificate'
  | 'jump_private_key'
  | 'jump_certificate';
export type DirPickerTarget = 'project' | 'terminal';

export interface RemoteConnectionFormPayload {
  name?: string;
  host: string;
  port?: number;
  username: string;
  auth_type?: RemoteAuthType;
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: HostKeyPolicy;
  jump_enabled?: boolean;
  jump_connection_id?: string;
  jump_host?: string;
  jump_port?: number;
  jump_username?: string;
  jump_private_key_path?: string;
  jump_certificate_path?: string;
  jump_password?: string;
}

export interface RemoteConnectionFormValues {
  name: string;
  host: string;
  port: string;
  username: string;
  authType: RemoteAuthType;
  password: string;
  privateKeyPath: string;
  certificatePath: string;
  defaultPath: string;
  hostKeyPolicy: HostKeyPolicy;
  jumpEnabled: boolean;
  jumpMode: JumpHostMode;
  jumpConnectionId: string;
  jumpHost: string;
  jumpPort: string;
  jumpUsername: string;
  jumpPrivateKeyPath: string;
  jumpCertificatePath: string;
  jumpPassword: string;
}

export const formatTimeAgo = (date: string | Date | undefined | null) => {
  const now = new Date();
  let past: Date;

  if (!date) {
    return '时间未知';
  }

  if (typeof date === 'string') {
    const isoString = date.replace(' ', 'T') + 'Z';
    past = new Date(isoString);
    if (isNaN(past.getTime())) {
      past = new Date(date);
    }
  } else {
    past = date;
  }

  if (!past || isNaN(past.getTime())) {
    return '时间未知';
  }

  const diffInSeconds = Math.floor((now.getTime() - past.getTime()) / 1000);

  if (diffInSeconds < 60) return '刚刚';
  if (diffInSeconds < 3600) return `${Math.floor(diffInSeconds / 60)}分钟前`;
  if (diffInSeconds < 86400) return `${Math.floor(diffInSeconds / 3600)}小时前`;
  if (diffInSeconds < 2592000) return `${Math.floor(diffInSeconds / 86400)}天前`;
  return past.toLocaleDateString('zh-CN');
};

export const getSessionStatus = (session: Session): 'active' | 'archiving' | 'archived' => {
  const rawStatus = typeof session.status === 'string' ? session.status.toLowerCase() : '';
  if (rawStatus === 'archiving') return 'archiving';
  if (rawStatus === 'archived') return 'archived';
  if (session.archived) return 'archived';
  return 'active';
};

export const deriveNameFromPath = (path: string, fallback: string): string => {
  const trimmed = path.trim().replace(/[\\/]+$/, '');
  if (!trimmed) return fallback;
  const parts = trimmed.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || fallback;
};

export const deriveParentPath = (path: string): string | null => {
  const trimmed = path.trim();
  if (/^[A-Za-z]:[\\/]?$/.test(trimmed)) {
    return `${trimmed.slice(0, 2)}\\`;
  }
  const normalized = path.trim().replace(/[\\/]+$/, '');
  if (!normalized) return null;
  const idx = Math.max(normalized.lastIndexOf('/'), normalized.lastIndexOf('\\'));
  if (idx < 0) return null;
  if (idx === 0) return normalized[0];
  const parent = normalized.slice(0, idx);
  if (/^[A-Za-z]:$/.test(parent)) {
    return `${parent}\\`;
  }
  return parent;
};

export const normalizeFsEntry = (raw: any, fallbackIsDir: boolean): FsEntry => ({
  name: raw?.name ?? '',
  path: raw?.path ?? '',
  isDir: raw?.is_dir ?? raw?.isDir ?? fallbackIsDir,
  size: raw?.size ?? null,
  modifiedAt: raw?.modified_at ?? raw?.modifiedAt ?? null,
});

export const getKeyFilePickerTitle = (target: KeyFilePickerTarget): string => {
  if (target === 'private_key') {
    return '选择私钥文件';
  }
  if (target === 'certificate') {
    return '选择证书文件';
  }
  if (target === 'jump_certificate') {
    return '选择跳板机证书文件';
  }
  return '选择跳板机私钥文件';
};

export const buildRemoteConnectionPayload = (
  values: RemoteConnectionFormValues,
  availableRemoteConnections: RemoteConnection[] = [],
  editingRemoteConnectionId?: string | null,
): { payload: RemoteConnectionFormPayload } | { error: string } => {
  const {
    name,
    host,
    port,
    username,
    authType,
    password,
    privateKeyPath,
    certificatePath,
    defaultPath,
    hostKeyPolicy,
    jumpEnabled,
    jumpMode,
    jumpConnectionId,
    jumpHost,
    jumpPort,
    jumpUsername,
    jumpPrivateKeyPath,
    jumpCertificatePath,
    jumpPassword,
  } = values;

  if (!host.trim()) {
    return { error: '请输入主机地址' };
  }
  if (!username.trim()) {
    return { error: '请输入用户名' };
  }
  if (authType === 'password' && !password.trim()) {
    return { error: '密码模式需要填写密码' };
  }
  if (authType !== 'password' && !privateKeyPath.trim()) {
    return { error: '请输入私钥路径' };
  }
  if (authType === 'private_key_cert' && !certificatePath.trim()) {
    return { error: '私钥+证书模式需要证书路径' };
  }
  const normalizedJumpConnectionId = jumpConnectionId.trim();
  const selectedJumpConnection = availableRemoteConnections.find(
    (item) => item.id === normalizedJumpConnectionId,
  );

  if (jumpEnabled && jumpMode === 'existing' && !selectedJumpConnection) {
    return { error: '请选择已有远端连接作为跳板机' };
  }
  if (
    jumpEnabled
    && jumpMode === 'existing'
    && editingRemoteConnectionId
    && normalizedJumpConnectionId === editingRemoteConnectionId
  ) {
    return { error: '跳板机不能选择当前正在编辑的连接' };
  }
  if (jumpEnabled && jumpMode === 'manual' && (!jumpHost.trim() || !jumpUsername.trim())) {
    return { error: '启用跳板机后需填写跳板机主机和用户名' };
  }
  if (jumpEnabled && jumpMode === 'manual' && jumpCertificatePath.trim() && !jumpPrivateKeyPath.trim()) {
    return { error: '跳板机证书模式需要先填写跳板机私钥路径' };
  }

  const parsedPort = Number(port);
  if (!Number.isFinite(parsedPort) || parsedPort < 1 || parsedPort > 65535) {
    return { error: '端口范围必须在 1-65535' };
  }
  const effectiveJumpPort = jumpMode === 'existing'
    ? Number(selectedJumpConnection?.port ?? 22)
    : Number(jumpPort);
  if (jumpEnabled && (!Number.isFinite(effectiveJumpPort) || effectiveJumpPort < 1 || effectiveJumpPort > 65535)) {
    return { error: '跳板机端口范围必须在 1-65535' };
  }
  const effectiveJumpHost = jumpMode === 'existing'
    ? selectedJumpConnection?.host
    : jumpHost.trim();
  const effectiveJumpUsername = jumpMode === 'existing'
    ? selectedJumpConnection?.username
    : jumpUsername.trim();
  const effectiveJumpPrivateKeyPath = jumpMode === 'existing'
    ? undefined
    : jumpPrivateKeyPath.trim();
  const effectiveJumpCertificatePath = jumpMode === 'existing'
    ? undefined
    : jumpCertificatePath.trim();
  const effectiveJumpPassword = jumpMode === 'existing'
    ? undefined
    : jumpPassword;

  const defaultName = `${username.trim()}@${host.trim()}`;
  return {
    payload: {
      name: name.trim() || defaultName,
      host: host.trim(),
      port: parsedPort,
      username: username.trim(),
      auth_type: authType,
      password: authType === 'password' ? password : undefined,
      private_key_path: authType === 'password' ? undefined : privateKeyPath.trim(),
      certificate_path: authType === 'private_key_cert' ? certificatePath.trim() : undefined,
      default_remote_path: defaultPath.trim() || undefined,
      host_key_policy: hostKeyPolicy,
      jump_enabled: jumpEnabled,
      jump_connection_id:
        jumpEnabled && jumpMode === 'existing' ? normalizedJumpConnectionId : undefined,
      jump_host: jumpEnabled ? effectiveJumpHost : undefined,
      jump_port: jumpEnabled ? effectiveJumpPort : undefined,
      jump_username: jumpEnabled ? effectiveJumpUsername : undefined,
      jump_private_key_path:
        jumpEnabled && effectiveJumpPrivateKeyPath?.trim()
          ? effectiveJumpPrivateKeyPath.trim()
          : undefined,
      jump_certificate_path:
        jumpEnabled && effectiveJumpCertificatePath?.trim()
          ? effectiveJumpCertificatePath.trim()
          : undefined,
      jump_password: jumpEnabled && effectiveJumpPassword?.trim()
        ? effectiveJumpPassword.trim()
        : undefined,
    },
  };
};
