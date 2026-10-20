import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { remoteDirectoryFixture } from "../../../test/remoteFileFixture";
import { TauriRemoteFileAdapter } from "./TauriRemoteFileAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriRemoteFileAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("passes the typed operation envelope without exposing local paths", async () => {
    vi.mocked(invoke).mockResolvedValue({ status: "completed", listing: remoteDirectoryFixture() });
    await new TauriRemoteFileAdapter().listDirectory({
      ownerUserId: "alice",
      input: { operationId: "remote-file-1", connectionId: "remote-1", verificationCode: null },
      path: "/srv/app",
    });
    expect(invoke).toHaveBeenCalledWith("list_remote_directory", {
      ownerUserId: "alice",
      input: { operationId: "remote-file-1", connectionId: "remote-1", verificationCode: null },
      path: "/srv/app",
    });
  });

  it("rejects an unexpected absolute path returned by a native picker", async () => {
    vi.mocked(invoke).mockResolvedValue({
      selectionId: "remote-file-selection-1",
      fileName: "app.zip",
      size: 10,
      absolutePath: "C:\\Users\\alice\\app.zip",
    });
    await expect(new TauriRemoteFileAdapter().pickUploadFile({
      ownerUserId: "alice",
      presentationId: "remote-files-1",
    })).rejects.toThrow();
  });
});
