import type { RemoteConnection } from '../../types';
import type { RemoteConnectionResponse } from '../api/client/types';
import {
  asRecord,
  normalizeDate,
  readValue,
} from './normalizerUtils';

export const normalizeRemoteConnection = (raw: RemoteConnectionResponse | unknown): RemoteConnection => {
  const record = asRecord(raw);
  const createdAtSource = readValue(record, 'created_at') ?? readValue(record, 'createdAt') ?? Date.now();
  const updatedAtSource = readValue(record, 'updated_at')
    ?? readValue(record, 'updatedAt')
    ?? createdAtSource;
  const lastActiveAtSource = readValue(record, 'last_active_at')
    ?? readValue(record, 'lastActiveAt')
    ?? updatedAtSource;

  return {
    id: (readValue(record, 'id') ?? '') as RemoteConnection['id'],
    name: (readValue(record, 'name') ?? '') as RemoteConnection['name'],
    host: (readValue(record, 'host') ?? '') as RemoteConnection['host'],
    port: Number(readValue(record, 'port') ?? 22),
    username: (readValue(record, 'username') ?? '') as RemoteConnection['username'],
    authType: (readValue(record, 'auth_type') ?? readValue(record, 'authType') ?? 'private_key') as RemoteConnection['authType'],
    password: (readValue(record, 'password') ?? null) as RemoteConnection['password'],
    privateKeyPath: (readValue(record, 'private_key_path') ?? readValue(record, 'privateKeyPath') ?? null) as RemoteConnection['privateKeyPath'],
    certificatePath: (readValue(record, 'certificate_path') ?? readValue(record, 'certificatePath') ?? null) as RemoteConnection['certificatePath'],
    defaultRemotePath: (readValue(record, 'default_remote_path') ?? readValue(record, 'defaultRemotePath') ?? null) as RemoteConnection['defaultRemotePath'],
    hostKeyPolicy: (readValue(record, 'host_key_policy') ?? readValue(record, 'hostKeyPolicy') ?? 'strict') as RemoteConnection['hostKeyPolicy'],
    jumpEnabled: Boolean(readValue(record, 'jump_enabled') ?? readValue(record, 'jumpEnabled') ?? false),
    jumpConnectionId: (readValue(record, 'jump_connection_id') ?? readValue(record, 'jumpConnectionId') ?? null) as RemoteConnection['jumpConnectionId'],
    jumpHost: (readValue(record, 'jump_host') ?? readValue(record, 'jumpHost') ?? null) as RemoteConnection['jumpHost'],
    jumpPort: (readValue(record, 'jump_port') ?? readValue(record, 'jumpPort') ?? null) as RemoteConnection['jumpPort'],
    jumpUsername: (readValue(record, 'jump_username') ?? readValue(record, 'jumpUsername') ?? null) as RemoteConnection['jumpUsername'],
    jumpPrivateKeyPath: (readValue(record, 'jump_private_key_path') ?? readValue(record, 'jumpPrivateKeyPath') ?? null) as RemoteConnection['jumpPrivateKeyPath'],
    jumpCertificatePath: (readValue(record, 'jump_certificate_path') ?? readValue(record, 'jumpCertificatePath') ?? null) as RemoteConnection['jumpCertificatePath'],
    jumpPassword: (readValue(record, 'jump_password') ?? readValue(record, 'jumpPassword') ?? null) as RemoteConnection['jumpPassword'],
    userId: (readValue(record, 'user_id') ?? readValue(record, 'userId') ?? null) as RemoteConnection['userId'],
    createdAt: normalizeDate(createdAtSource),
    updatedAt: normalizeDate(updatedAtSource),
    lastActiveAt: normalizeDate(lastActiveAtSource),
  };
};
