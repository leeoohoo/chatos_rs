import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RemoteConnection } from "../domain/remoteConnection";
import type { RemoteConnectionPort } from "../ports/RemoteConnectionPort";
import { useRemoteConnectionStore } from "./remoteConnectionStore";

describe("remote connection store", () => {
  beforeEach(() => useRemoteConnectionStore.getState().reset());

  it("sorts the macOS source-of-truth list and keeps the last trusted snapshot on refresh failure", async () => {
    const adapter = port({
      list: vi.fn()
        .mockResolvedValueOnce([fixture({ id: "b", name: "Server 10" }), fixture({ id: "a", name: "Server 2" })])
        .mockRejectedValueOnce(new Error("Host restarting")),
    });
    await useRemoteConnectionStore.getState().activate(adapter, "alice");
    expect(useRemoteConnectionStore.getState().connections.map((connection) => connection.id)).toEqual(["a", "b"]);
    await useRemoteConnectionStore.getState().refresh(adapter);
    expect(useRemoteConnectionStore.getState().connections.map((connection) => connection.id)).toEqual(["a", "b"]);
    expect(useRemoteConnectionStore.getState().errorMessage).toBe("Host restarting");
  });

  it("rejects a late list after the account changes", async () => {
    const alice = deferred<RemoteConnection[]>();
    const adapter = port({
      list: vi.fn(async ({ ownerUserId }) => ownerUserId === "alice"
        ? alice.promise
        : [fixture({ id: "bob-remote", name: "Bob" })]),
    });
    const stale = useRemoteConnectionStore.getState().activate(adapter, "alice");
    await useRemoteConnectionStore.getState().activate(adapter, "bob");
    alice.resolve([fixture({ id: "alice-remote", name: "Alice" })]);
    await stale;
    expect(useRemoteConnectionStore.getState().ownerUserId).toBe("bob");
    expect(useRemoteConnectionStore.getState().connections.map((connection) => connection.id)).toEqual(["bob-remote"]);
  });

  it("rejects a late list after Host suspension while retaining the last trusted list", async () => {
    const late = deferred<RemoteConnection[]>();
    const adapter = port({ list: vi.fn(() => late.promise) });
    useRemoteConnectionStore.setState({
      ownerUserId: "alice",
      hostAvailable: false,
      connections: [fixture()],
    });
    const loading = useRemoteConnectionStore.getState().activate(adapter, "alice");
    useRemoteConnectionStore.getState().suspend("alice");
    late.resolve([fixture({ id: "late", name: "Late" })]);
    await loading;
    expect(useRemoteConnectionStore.getState().connections.map((connection) => connection.id)).toEqual(["remote-1"]);
    expect(useRemoteConnectionStore.getState().isLoading).toBe(false);
  });

  it("accepts committed create/update/delete results only for the active owner", async () => {
    const adapter = port();
    await useRemoteConnectionStore.getState().activate(adapter, "alice");
    useRemoteConnectionStore.getState().acceptSaved("bob", fixture({ id: "wrong", name: "Wrong" }));
    useRemoteConnectionStore.getState().acceptSaved("alice", fixture({ id: "new", name: "Alpha" }));
    expect(useRemoteConnectionStore.getState().connections.map((connection) => connection.id)).toEqual(["new", "remote-1"]);
    useRemoteConnectionStore.getState().acceptDeleted("bob", "remote-1");
    useRemoteConnectionStore.getState().acceptDeleted("alice", "remote-1");
    expect(useRemoteConnectionStore.getState().connections.map((connection) => connection.id)).toEqual(["new"]);
  });
});

function port(overrides: Partial<RemoteConnectionPort> = {}): RemoteConnectionPort {
  return {
    list: vi.fn(async () => [fixture()]),
    get: vi.fn(async () => fixture()),
    create: vi.fn(async () => fixture()),
    update: vi.fn(async () => fixture()),
    delete: vi.fn(async () => "remote-1"),
    testDraft: vi.fn(async () => ({
      success: true,
      verificationRequired: false,
      message: "连接成功",
      verificationPrompt: null,
    })),
    testSaved: vi.fn(async () => ({
      success: true,
      verificationRequired: false,
      message: "连接成功",
      verificationPrompt: null,
    })),
    ...overrides,
  };
}

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

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
