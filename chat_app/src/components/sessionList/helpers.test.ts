import { describe, expect, it } from 'vitest';

import {
  buildRemoteConnectionPayload,
  deriveParentPath,
  type RemoteConnectionFormValues,
} from './helpers';

const buildFormValues = (
  overrides: Partial<RemoteConnectionFormValues> = {},
): RemoteConnectionFormValues => ({
  name: '',
  host: 'example.com',
  port: '22',
  username: 'root',
  authType: 'private_key',
  password: '',
  privateKeyPath: '/tmp/id_rsa',
  certificatePath: '',
  defaultPath: '/srv/app',
  hostKeyPolicy: 'strict',
  jumpEnabled: false,
  jumpHost: '',
  jumpPort: '22',
  jumpUsername: '',
  jumpPrivateKeyPath: '',
  jumpPassword: '',
  ...overrides,
});

describe('sessionList helpers', () => {
  it('builds normalized remote connection payload', () => {
    const result = buildRemoteConnectionPayload(
      buildFormValues({
        name: '  生产机器  ',
        host: ' example.com ',
        username: ' deploy ',
        defaultPath: ' /srv/app ',
      }),
    );

    expect('payload' in result).toBe(true);
    if (!('payload' in result)) {
      return;
    }

    expect(result.payload).toEqual({
      name: '生产机器',
      host: 'example.com',
      port: 22,
      username: 'deploy',
      auth_type: 'private_key',
      password: undefined,
      private_key_path: '/tmp/id_rsa',
      certificate_path: undefined,
      default_remote_path: '/srv/app',
      host_key_policy: 'strict',
      jump_enabled: false,
      jump_host: undefined,
      jump_port: undefined,
      jump_username: undefined,
      jump_private_key_path: undefined,
      jump_password: undefined,
    });
  });

  it('rejects invalid jump host configuration', () => {
    const result = buildRemoteConnectionPayload(
      buildFormValues({
        jumpEnabled: true,
        jumpHost: '',
        jumpUsername: 'jump-user',
      }),
    );

    expect(result).toEqual({ error: '启用跳板机后需填写跳板机主机和用户名' });
  });

  it('derives parent paths for unix and windows roots', () => {
    expect(deriveParentPath('/srv/app/src')).toBe('/srv/app');
    expect(deriveParentPath('C:\\workspace\\demo')).toBe('C:\\workspace');
    expect(deriveParentPath('C:\\')).toBe('C:\\');
  });
});
