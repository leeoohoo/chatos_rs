import type { RemoteConnection } from "../features/remote-connection/domain/remoteConnection";

export function remoteConnectionFixture(
  overrides: Partial<RemoteConnection> = {},
): RemoteConnection {
  return {
    id: "remote-1",
    name: "Production",
    host: "server.example.com",
    port: 22,
    username: "deploy",
    authenticationType: "private_key",
    hasPassword: false,
    hasPrivateKeyPath: true,
    hasCertificatePath: false,
    defaultRemotePath: "/srv/app",
    hostKeyPolicy: "strict",
    jumpEnabled: false,
    jumpConnectionId: null,
    jumpHost: null,
    jumpPort: null,
    jumpUsername: null,
    hasJumpPrivateKeyPath: false,
    hasJumpCertificatePath: false,
    hasJumpPassword: false,
    lastActiveAtUnixMs: null,
    version: 1,
    createdAtUnixMs: 1_000,
    updatedAtUnixMs: 2_000,
    ...overrides,
  };
}
