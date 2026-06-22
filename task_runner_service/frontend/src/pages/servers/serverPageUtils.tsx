import { Tag } from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  CreateRemoteServerPayload,
  RemoteServerAuthType,
  RemoteServerRecord,
  TestRemoteServerPayload,
} from '../../types';

export type RemoteServerFormValues = {
  name: string;
  host: string;
  port?: number;
  username: string;
  auth_type: RemoteServerAuthType;
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy: 'accept_new' | 'strict';
  enabled: boolean;
};

export type ServerEnabledFilter = 'all' | 'enabled' | 'disabled';

export const authTypeLabelKeys: Record<RemoteServerAuthType, string> = {
  password: 'servers.auth.password',
  private_key: 'servers.auth.privateKey',
  private_key_cert: 'servers.auth.privateKeyCert',
};

export const HOST_KEY_POLICY_OPTIONS = [
  { label: 'accept_new', value: 'accept_new' },
  { label: 'strict', value: 'strict' },
] as const;

export function serverCreatorLabel(server: RemoteServerRecord): string {
  return server.creator_display_name || server.creator_username || server.creator_user_id || '-';
}

export function getAuthTypeLabel(value: string, t: TranslateFn): string {
  return value in authTypeLabelKeys
    ? t(authTypeLabelKeys[value as RemoteServerAuthType])
    : value;
}

export function buildRemoteServerPayload(
  values: RemoteServerFormValues,
): CreateRemoteServerPayload {
  const base = {
    name: values.name,
    host: values.host,
    port: values.port,
    username: values.username,
    auth_type: values.auth_type,
    default_remote_path: values.default_remote_path || '',
    host_key_policy: values.host_key_policy,
    enabled: values.enabled,
  };

  if (values.auth_type === 'password') {
    return {
      ...base,
      password: values.password || '',
      private_key_path: '',
      certificate_path: '',
    };
  }

  if (values.auth_type === 'private_key') {
    return {
      ...base,
      password: '',
      private_key_path: values.private_key_path || '',
      certificate_path: '',
    };
  }

  return {
    ...base,
    password: '',
    private_key_path: values.private_key_path || '',
    certificate_path: values.certificate_path || '',
  };
}

export function buildRemoteServerTestPayload(
  values: RemoteServerFormValues,
): TestRemoteServerPayload {
  const payload = buildRemoteServerPayload(values);
  return {
    ...payload,
  };
}

export function normalizeAuthType(value: string): RemoteServerAuthType {
  if (value === 'private_key' || value === 'private_key_cert') {
    return value;
  }
  return 'password';
}

export function normalizeHostKeyPolicy(value: string): 'accept_new' | 'strict' {
  return value === 'strict' ? 'strict' : 'accept_new';
}

export function renderTestStatus(value: string | null | undefined, t: TranslateFn) {
  if (value === 'success') {
    return <Tag color="success">{t('common.success')}</Tag>;
  }
  if (value === 'failed') {
    return <Tag color="error">{t('common.failed')}</Tag>;
  }
  return <Tag>{t('servers.untested')}</Tag>;
}
