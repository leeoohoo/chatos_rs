import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RemoteCredentialFileSelection } from "../domain/remoteConnection";
import type { RemoteConnectionPort } from "../ports/RemoteConnectionPort";
import { remoteConnectionFixture } from "../../../test/remoteConnectionFixture";
import { useRemoteConnectionEditorStore } from "./remoteConnectionEditorStore";

describe("remote connection editor store", () => {
  beforeEach(() => {
    useRemoteConnectionEditorStore.setState({
      ownerUserId: null,
      presentationId: null,
      editingConnection: null,
      connections: [],
      generation: 0,
      requestSequence: 0,
      activeOperation: null,
      successMessage: null,
      errorMessage: null,
      verificationPrompt: null,
      verificationCode: "",
    });
  });

  it("rejects a late native picker result after the editor closes", async () => {
    const picked = deferred<RemoteCredentialFileSelection | null>();
    const adapter = port({ pickCredentialFile: vi.fn(() => picked.promise) });
    useRemoteConnectionEditorStore.getState().open(adapter, "alice", null, []);
    const presentationId = useRemoteConnectionEditorStore.getState().presentationId!;
    const selecting = useRemoteConnectionEditorStore.getState().pickCredentialFile(adapter, "private_key");
    useRemoteConnectionEditorStore.getState().close(adapter);
    picked.resolve({
      selectionId: "remote-credential-file-late",
      fileName: "late-key",
      kind: "private_key",
    });
    await selecting;
    expect(useRemoteConnectionEditorStore.getState().ownerUserId).toBeNull();
    expect(adapter.discardCredentialFileSelections).toHaveBeenCalledWith({
      ownerUserId: "alice",
      presentationId,
    });
  });

  it("keeps draft testing single-flight and resumes MFA with the same presentation", async () => {
    const editing = remoteConnectionFixture();
    const first = deferred<Awaited<ReturnType<RemoteConnectionPort["testDraft"]>>>();
    const testDraft = vi.fn()
      .mockImplementationOnce(() => first.promise)
      .mockResolvedValueOnce({
        success: true,
        verificationRequired: false,
        message: "连接成功",
        verificationPrompt: null,
      });
    const adapter = port({ testDraft });
    useRemoteConnectionEditorStore.getState().open(adapter, "alice", editing, [editing]);
    const testing = useRemoteConnectionEditorStore.getState().testConnection(adapter);
    expect(await useRemoteConnectionEditorStore.getState().save(adapter)).toBeNull();
    first.resolve({
      success: false,
      verificationRequired: true,
      message: "需要 SSH 二次验证。",
      verificationPrompt: "OTP code:",
    });
    await testing;
    expect(useRemoteConnectionEditorStore.getState().verificationPrompt).toBe("OTP code:");
    await useRemoteConnectionEditorStore.getState().testConnection(adapter, " 123456 ");
    expect(testDraft).toHaveBeenLastCalledWith(expect.objectContaining({
      ownerUserId: "alice",
      credentialReferenceId: "remote-1",
      verificationCode: "123456",
    }));
    expect(useRemoteConnectionEditorStore.getState().successMessage).toBe("连接成功");
  });

  it("returns the committed connection only to the active presentation", async () => {
    const saving = deferred<ReturnType<typeof remoteConnectionFixture>>();
    const adapter = port({ update: vi.fn(() => saving.promise) });
    const editing = remoteConnectionFixture();
    useRemoteConnectionEditorStore.getState().open(adapter, "alice", editing, [editing]);
    const result = useRemoteConnectionEditorStore.getState().save(adapter);
    useRemoteConnectionEditorStore.getState().close(adapter);
    saving.resolve(remoteConnectionFixture({ version: 2, name: "Updated" }));
    expect(await result).toBeNull();
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
