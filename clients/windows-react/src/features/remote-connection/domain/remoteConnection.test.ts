import { describe, expect, it } from "vitest";
import {
  remoteConnectionSchema,
  remoteConnectionTestResultSchema,
  sortRemoteConnections,
  type RemoteConnection,
} from "./remoteConnection";

describe("remote connection domain", () => {
  it("accepts only the safe native projection without stored credential values", () => {
    const connection = remoteConnectionSchema.parse(fixture());
    expect(connection).toMatchObject({
      id: "remote-1",
      hasPrivateKeyPath: true,
      hasCertificatePath: false,
    });
    expect(JSON.stringify(connection)).not.toContain("C:\\\\Users");
    expect(JSON.stringify(connection)).not.toContain("PRIVATE KEY");
  });

  it("rejects jump state when jump routing is disabled", () => {
    expect(() => remoteConnectionSchema.parse({
      ...fixture(),
      jumpEnabled: false,
      jumpHost: "bastion.example.com",
    })).toThrow();
  });

  it("requires verification prompts to match the test state", () => {
    expect(() => remoteConnectionTestResultSchema.parse({
      success: false,
      verificationRequired: true,
      message: "需要验证",
      verificationPrompt: null,
    })).toThrow();
  });

  it("sorts by the macOS list contract without mutating the input", () => {
    const original = [fixture({ id: "b", name: "Server 10" }), fixture({ id: "a", name: "Server 2" })];
    const sorted = sortRemoteConnections(original);
    expect(sorted.map((connection) => connection.id)).toEqual(["a", "b"]);
    expect(original.map((connection) => connection.id)).toEqual(["b", "a"]);
  });
});

function fixture(overrides: Partial<RemoteConnection> = {}): RemoteConnection {
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
