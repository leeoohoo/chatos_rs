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
  jumpMode: 'manual',
  jumpConnectionId: '',
  jumpHost: '',
  jumpPort: '22',
  jumpUsername: '',
  jumpPrivateKeyPath: '',
  jumpCertificatePath: '',
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
      jump_connection_id: undefined,
      jump_host: undefined,
      jump_port: undefined,
      jump_username: undefined,
      jump_private_key_path: undefined,
      jump_certificate_path: undefined,
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

  it('copies an existing remote connection snapshot for jump host mode', () => {
    const result = buildRemoteConnectionPayload(
      buildFormValues({
        jumpEnabled: true,
        jumpMode: 'existing',
        jumpConnectionId: 'jump-1',
      }),
      [{
        id: 'jump-1',
        name: 'jump-box',
        host: 'jump.example.com',
        port: 2222,
        username: 'jump-user',
        authType: 'private_key_cert',
        password: null,
        privateKeyPath: '/tmp/jump_id_rsa',
        certificatePath: '/tmp/jump_id_rsa-cert.pub',
        defaultRemotePath: null,
        hostKeyPolicy: 'strict',
        jumpEnabled: false,
        userId: null,
        createdAt: new Date('2026-01-01T00:00:00Z'),
        updatedAt: new Date('2026-01-01T00:00:00Z'),
        lastActiveAt: new Date('2026-01-01T00:00:00Z'),
      }],
    );

    expect('payload' in result).toBe(true);
    if (!('payload' in result)) {
      return;
    }

    expect(result.payload.jump_connection_id).toBe('jump-1');
    expect(result.payload.jump_host).toBe('jump.example.com');
    expect(result.payload.jump_port).toBe(2222);
    expect(result.payload.jump_username).toBe('jump-user');
    expect(result.payload.jump_private_key_path).toBeUndefined();
    expect(result.payload.jump_certificate_path).toBeUndefined();
    expect(result.payload.jump_password).toBeUndefined();
  });

  it('keeps manual jump certificate when provided', () => {
    const result = buildRemoteConnectionPayload(
      buildFormValues({
        jumpEnabled: true,
        jumpMode: 'manual',
        jumpHost: 'jump.example.com',
        jumpUsername: 'jump-user',
        jumpPrivateKeyPath: '/tmp/jump_id_rsa',
        jumpCertificatePath: '/tmp/jump_id_rsa-cert.pub',
      }),
    );

    expect('payload' in result).toBe(true);
    if (!('payload' in result)) {
      return;
    }

    expect(result.payload.jump_private_key_path).toBe('/tmp/jump_id_rsa');
    expect(result.payload.jump_certificate_path).toBe('/tmp/jump_id_rsa-cert.pub');
  });

  it('derives parent paths for unix and windows roots', () => {
    expect(deriveParentPath('/srv/app/src')).toBe('/srv/app');
    expect(deriveParentPath('C:\\workspace\\demo')).toBe('C:\\workspace');
    expect(deriveParentPath('C:\\')).toBe('C:\\');
  });
});
