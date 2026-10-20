import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RemoteConnectionPort } from "../ports/RemoteConnectionPort";
import { remoteConnectionFixture } from "../../../test/remoteConnectionFixture";
import { useRemoteConnectionDetailStore } from "./remoteConnectionDetailStore";

describe("remote connection detail store", () => {
  beforeEach(() => {
    useRemoteConnectionDetailStore.setState({
      ownerUserId: null,
      presentationId: null,
      connection: null,
      generation: 0,
      requestSequence: 0,
      activeOperation: null,
      notice: null,
      errorMessage: null,
      verificationPrompt: null,
      verificationCode: "",
      deletedConnectionId: null,
    });
  });

  it("continues saved-connection MFA without allowing a concurrent delete", async () => {
    const challenge = deferred<Awaited<ReturnType<RemoteConnectionPort["testSaved"]>>>();
    const testSaved = vi.fn()
      .mockImplementationOnce(() => challenge.promise)
      .mockResolvedValueOnce({
        success: true,
        verificationRequired: false,
        message: "连接成功",
        verificationPrompt: null,
      });
    const adapter = port({ testSaved });
    useRemoteConnectionDetailStore.getState().activate(adapter, "alice", remoteConnectionFixture());
    const testing = useRemoteConnectionDetailStore.getState().testSavedConnection(adapter);
    expect(await useRemoteConnectionDetailStore.getState().deleteConnection(adapter)).toBeNull();
    challenge.resolve({
      success: false,
      verificationRequired: true,
      message: "需要 SSH 二次验证。",
      verificationPrompt: "OTP code:",
    });
    await testing;
    await useRemoteConnectionDetailStore.getState().testSavedConnection(adapter, " 654321 ");
    expect(testSaved).toHaveBeenLastCalledWith({
      ownerUserId: "alice",
      connectionId: "remote-1",
      verificationCode: "654321",
    });
    expect(useRemoteConnectionDetailStore.getState().notice).toBe("连接成功");
  });

  it("deletes with the displayed version and rejects a late result after deactivation", async () => {
    const deleted = deferred<string>();
    const adapter = port({ delete: vi.fn(() => deleted.promise) });
    useRemoteConnectionDetailStore.getState().activate(
      adapter,
      "alice",
      remoteConnectionFixture({ version: 7 }),
    );
    const deleting = useRemoteConnectionDetailStore.getState().deleteConnection(adapter);
    expect(adapter.delete).toHaveBeenCalledWith({
      ownerUserId: "alice",
      connectionId: "remote-1",
      expectedVersion: 7,
    });
    useRemoteConnectionDetailStore.getState().deactivate(adapter);
    deleted.resolve("remote-1");
    expect(await deleting).toBeNull();
    expect(useRemoteConnectionDetailStore.getState().deletedConnectionId).toBeNull();
  });
});

function port(overrides: Partial<RemoteConnectionPort> = {}): RemoteConnectionPort {
  return {
    list: vi.fn(async () => [remoteConnectionFixture()]),
    get: vi.fn(async () => remoteConnectionFixture()),
    create: vi.fn(async () => remoteConnectionFixture()),
    update: vi.fn(async () => remoteConnectionFixture({ version: 2 })),
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
    pickCredentialFile: vi.fn(async () => null),
    discardCredentialFileSelections: vi.fn(async () => undefined),
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
