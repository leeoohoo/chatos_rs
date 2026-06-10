import type { FsEntry, RemoteConnection, Session } from '../../types';
import type { FsEntryResponse } from '../../lib/api/client/types';
import {
  normalizeFsEntry as normalizeDomainFsEntry,
} from '../../lib/domain/filesystem';
import type { TranslateFn } from '../../i18n/I18nProvider';
import { UI_MESSAGES } from '../../i18n/messages';

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

const formatFallbackMessage = (
  template: string,
  params?: Record<string, string | number>,
): string => {
  if (!params) {
    return template;
  }
  return template.replace(/\{(\w+)\}/g, (_match, key: string) => (
    Object.prototype.hasOwnProperty.call(params, key)
      ? String(params[key])
      : `{${key}}`
  ));
};

export const translateSessionListMessage = (
  t: TranslateFn | undefined,
  key: string,
  params?: Record<string, string | number>,
): string => (
  t ? t(key, params) : formatFallbackMessage(UI_MESSAGES['zh-CN'][key] || key, params)
);

export const formatTimeAgo = (
  date: string | Date | undefined | null,
  t?: TranslateFn,
  locale = 'zh-CN',
) => {
  const now = new Date();
  let past: Date;

  if (!date) {
    return translateSessionListMessage(t, 'sessionList.time.unknown');
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
    return translateSessionListMessage(t, 'sessionList.time.unknown');
  }

  const diffInSeconds = Math.floor((now.getTime() - past.getTime()) / 1000);

  if (diffInSeconds < 60) return translateSessionListMessage(t, 'sessionList.time.justNow');
  if (diffInSeconds < 3600) {
    return translateSessionListMessage(t, 'sessionList.time.minutesAgo', { count: Math.floor(diffInSeconds / 60) });
  }
  if (diffInSeconds < 86400) {
    return translateSessionListMessage(t, 'sessionList.time.hoursAgo', { count: Math.floor(diffInSeconds / 3600) });
  }
  if (diffInSeconds < 2592000) {
    return translateSessionListMessage(t, 'sessionList.time.daysAgo', { count: Math.floor(diffInSeconds / 86400) });
  }
  return past.toLocaleDateString(locale === 'en-US' ? 'en-US' : 'zh-CN');
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

export const normalizeFsEntry = (raw: FsEntryResponse | unknown, fallbackIsDir: boolean): FsEntry => {
  return normalizeDomainFsEntry(raw, { fallbackIsDir });
};

export const getKeyFilePickerTitle = (target: KeyFilePickerTarget, t?: TranslateFn): string => {
  if (target === 'private_key') {
    return translateSessionListMessage(t, 'sessionList.keyFilePicker.privateKey');
  }
  if (target === 'certificate') {
    return translateSessionListMessage(t, 'sessionList.keyFilePicker.certificate');
  }
  if (target === 'jump_certificate') {
    return translateSessionListMessage(t, 'sessionList.keyFilePicker.jumpCertificate');
  }
  return translateSessionListMessage(t, 'sessionList.keyFilePicker.jumpPrivateKey');
};

export const buildRemoteConnectionPayload = (
  values: RemoteConnectionFormValues,
  availableRemoteConnections: RemoteConnection[] = [],
  editingRemoteConnectionId?: string | null,
  t?: TranslateFn,
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
    return { error: translateSessionListMessage(t, 'remoteConnection.error.hostRequired') };
  }
  if (!username.trim()) {
    return { error: translateSessionListMessage(t, 'remoteConnection.error.usernameRequired') };
  }
  if (authType === 'password' && !password.trim()) {
    return { error: translateSessionListMessage(t, 'remoteConnection.error.passwordRequired') };
  }
  if (authType !== 'password' && !privateKeyPath.trim()) {
    return { error: translateSessionListMessage(t, 'remoteConnection.error.privateKeyRequired') };
  }
  if (authType === 'private_key_cert' && !certificatePath.trim()) {
    return { error: translateSessionListMessage(t, 'remoteConnection.error.certificateRequired') };
  }
  const normalizedJumpConnectionId = jumpConnectionId.trim();
  const selectedJumpConnection = availableRemoteConnections.find(
    (item) => item.id === normalizedJumpConnectionId,
  );

  if (jumpEnabled && jumpMode === 'existing' && !selectedJumpConnection) {
    return { error: translateSessionListMessage(t, 'remoteConnection.error.jumpExistingRequired') };
  }
  if (
    jumpEnabled
    && jumpMode === 'existing'
    && editingRemoteConnectionId
    && normalizedJumpConnectionId === editingRemoteConnectionId
  ) {
    return { error: translateSessionListMessage(t, 'remoteConnection.error.jumpCannotUseSelf') };
  }
  if (jumpEnabled && jumpMode === 'manual' && (!jumpHost.trim() || !jumpUsername.trim())) {
    return { error: translateSessionListMessage(t, 'remoteConnection.error.jumpHostRequired') };
  }
  if (jumpEnabled && jumpMode === 'manual' && jumpCertificatePath.trim() && !jumpPrivateKeyPath.trim()) {
    return { error: translateSessionListMessage(t, 'remoteConnection.error.jumpCertificateNeedsKey') };
  }

  const parsedPort = Number(port);
  if (!Number.isFinite(parsedPort) || parsedPort < 1 || parsedPort > 65535) {
    return { error: translateSessionListMessage(t, 'remoteConnection.error.portRange') };
  }
  const effectiveJumpPort = jumpMode === 'existing'
    ? Number(selectedJumpConnection?.port ?? 22)
    : Number(jumpPort);
  if (jumpEnabled && (!Number.isFinite(effectiveJumpPort) || effectiveJumpPort < 1 || effectiveJumpPort > 65535)) {
    return { error: translateSessionListMessage(t, 'remoteConnection.error.jumpPortRange') };
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
