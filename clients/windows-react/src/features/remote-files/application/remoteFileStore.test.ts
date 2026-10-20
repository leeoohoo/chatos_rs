import { beforeEach, describe, expect, it, vi } from "vitest";
import { remoteDirectoryFixture } from "../../../test/remoteFileFixture";
import type { RemoteFilePort } from "../ports/RemoteFilePort";
import { useRemoteFileStore } from "./remoteFileStore";

describe("remote file store", () => {
  beforeEach(resetStore);

  it("continues MFA with the exact operation identity and applies the trusted listing", async () => {
    const listDirectory = vi.fn()
      .mockResolvedValueOnce({ status: "verification_required", prompt: "OTP code:" })
      .mockResolvedValueOnce({ status: "completed", listing: remoteDirectoryFixture() });
    const adapter = port({ listDirectory });
    await useRemoteFileStore.getState().activate(adapter, "alice", "remote-1");
    const firstInput = listDirectory.mock.calls[0][0].input;
    expect(useRemoteFileStore.getState().verificationPrompt).toBe("OTP code:");
    useRemoteFileStore.getState().setVerificationCode(" 123456 ");
    await useRemoteFileStore.getState().submitVerification(adapter);
    expect(listDirectory.mock.calls[1][0].input).toEqual({ ...firstInput, verificationCode: "123456" });
    expect(useRemoteFileStore.getState().listing?.path).toBe("/srv/app");
  });

  it("rejects a late listing after the workspace closes and discards its presentation", async () => {
    const late = deferred<Awaited<ReturnType<RemoteFilePort["listDirectory"]>>>();
    const adapter = port({ listDirectory: vi.fn(() => late.promise) });
    const activating = useRemoteFileStore.getState().activate(adapter, "alice", "remote-1");
    await vi.waitFor(() => expect(adapter.listDirectory).toHaveBeenCalledOnce());
    const presentationId = useRemoteFileStore.getState().presentationId;
    useRemoteFileStore.getState().deactivate(adapter);
    late.resolve({ status: "completed", listing: remoteDirectoryFixture() });
    await activating;
    expect(useRemoteFileStore.getState().listing).toBeNull();
    await vi.waitFor(() => expect(adapter.discardSelections).toHaveBeenCalledWith({ ownerUserId: "alice", presentationId }));
  });

  it("reports committed-without-refresh and blocks another operation in flight", async () => {
    const uploaded = deferred<Awaited<ReturnType<RemoteFilePort["uploadFile"]>>>();
    const adapter = port({
      pickUploadFile: vi.fn(async () => ({ selectionId: "remote-file-selection-upload", fileName: "bundle.zip", size: 5 })),
      uploadFile: vi.fn(() => uploaded.promise),
    });
    await useRemoteFileStore.getState().activate(adapter, "alice", "remote-1");
    await useRemoteFileStore.getState().pickUploadFile(adapter);
    const upload = useRemoteFileStore.getState().uploadSelected(adapter, false);
    await useRemoteFileStore.getState().loadDirectory(adapter, "/other");
    expect(adapter.listDirectory).toHaveBeenCalledOnce();
    uploaded.resolve({ status: "completed", result: { committed: true, refreshedListing: null, message: "已提交但刷新失败。" } });
    await upload;
    expect(useRemoteFileStore.getState().notice).toBe("已上传 bundle.zip");
    expect(useRemoteFileStore.getState().refreshWarning).toBe("已提交但刷新失败。");
    expect(useRemoteFileStore.getState().selectedUpload).toBeNull();
  });
});

function resetStore() {
  useRemoteFileStore.setState({
    ownerUserId: null,
    connectionId: null,
    presentationId: null,
    generation: 0,
    requestSequence: 0,
    listing: null,
    selectedRemotePath: null,
    selectedUpload: null,
    pendingDownload: null,
    searchText: "",
    showsHiddenFiles: false,
    activeOperation: null,
    notice: null,
    refreshWarning: null,
    errorMessage: null,
    verificationPrompt: null,
    verificationCode: "",
    pendingRetry: null,
  });
}

function port(overrides: Partial<RemoteFilePort> = {}): RemoteFilePort {
  const mutation = async () => ({ status: "completed" as const, result: { committed: true as const, refreshedListing: remoteDirectoryFixture(), message: null } });
  return {
    listDirectory: vi.fn(async () => ({ status: "completed" as const, listing: remoteDirectoryFixture() })),
    createDirectory: vi.fn(mutation),
    renameEntry: vi.fn(mutation),
    deleteEntry: vi.fn(mutation),
    pickUploadFile: vi.fn(async () => null),
    pickDownloadDestination: vi.fn(async () => null),
    uploadFile: vi.fn(mutation),
    downloadFile: vi.fn(async () => ({ status: "completed" as const, fileName: "app.log", size: 2_048 })),
    discardSelections: vi.fn(async () => undefined),
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
