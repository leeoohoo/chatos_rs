import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { remoteDirectoryFixture } from "../../../test/remoteFileFixture";
import type { RemoteFilePort } from "../ports/RemoteFilePort";
import { useRemoteFileStore } from "../application/remoteFileStore";
import { RemoteSftpWorkspace } from "./RemoteSftpWorkspace";

beforeEach(() => useRemoteFileStore.setState({ ownerUserId: null, connectionId: null, presentationId: null, listing: null, activeOperation: null }));
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("RemoteSftpWorkspace", () => {
  it("loads, filters, and navigates the real remote listing", async () => {
    const adapter = port();
    render(<RemoteSftpWorkspace connectionId="remote-1" ownerUserId="alice" port={adapter} />);
    await screen.findByRole("option", { name: /app.log/ });
    expect(adapter.listDirectory).toHaveBeenCalledWith(expect.objectContaining({ path: null }));
    expect(screen.queryByRole("option", { name: /.env/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "显示隐藏文件" }));
    expect(screen.getByRole("option", { name: /.env/ })).toBeVisible();
    fireEvent.doubleClick(screen.getByRole("option", { name: /releases/ }));
    await waitFor(() => expect(adapter.listDirectory).toHaveBeenLastCalledWith(expect.objectContaining({ path: "/srv/app/releases" })));
  });

  it("uses opaque selections for upload and confirms a destructive remote overwrite", async () => {
    const adapter = port({
      pickUploadFile: vi.fn(async () => ({ selectionId: "remote-file-selection-upload", fileName: "app.log", size: 2_048 })),
    });
    render(<RemoteSftpWorkspace connectionId="remote-1" ownerUserId="alice" port={adapter} />);
    await screen.findByRole("option", { name: /app.log/ });
    fireEvent.click(screen.getByRole("button", { name: "选择文件…" }));
    await screen.findByText("2.0 KB");
    fireEvent.click(screen.getByRole("button", { name: "上传到服务器 →" }));
    expect(screen.getByText("服务器上已存在“app.log”，继续将覆盖它。")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "覆盖" }));
    await waitFor(() => expect(adapter.uploadFile).toHaveBeenCalledWith(expect.objectContaining({
      ownerUserId: "alice",
      presentationId: expect.stringMatching(/^remote-files-/),
      selectionId: "remote-file-selection-upload",
      remoteDirectory: "/srv/app",
      overwrite: true,
    })));
    expect(JSON.stringify(vi.mocked(adapter.uploadFile).mock.calls)).not.toContain("C:\\");
  });

  it("requires explicit confirmation before deleting a server entry", async () => {
    const adapter = port();
    render(<RemoteSftpWorkspace connectionId="remote-1" ownerUserId="alice" port={adapter} />);
    const file = await screen.findByRole("option", { name: /app.log/ });
    fireEvent.click(file);
    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    expect(screen.getByText(/直接删除服务器上的“app.log”/)).toBeVisible();
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "删除" }));
    await waitFor(() => expect(adapter.deleteEntry).toHaveBeenCalledWith(expect.objectContaining({ path: "/srv/app/app.log", recursively: false })));
  });

  it("downloads through an opaque save selection and explicit overwrite confirmation", async () => {
    const adapter = port({
      pickDownloadDestination: vi.fn(async () => ({ selectionId: "remote-file-selection-download", fileName: "app.log", size: null })),
    });
    render(<RemoteSftpWorkspace connectionId="remote-1" ownerUserId="alice" port={adapter} />);
    fireEvent.click(await screen.findByRole("option", { name: /app.log/ }));
    fireEvent.click(screen.getByRole("button", { name: "← 下载到本机" }));
    await screen.findByText(/如果目标文件已存在，将直接覆盖/);
    fireEvent.click(screen.getByRole("button", { name: "保存并覆盖" }));
    await waitFor(() => expect(adapter.downloadFile).toHaveBeenCalledWith(expect.objectContaining({
      ownerUserId: "alice",
      presentationId: expect.stringMatching(/^remote-files-/),
      selectionId: "remote-file-selection-download",
      remotePath: "/srv/app/app.log",
      overwrite: true,
    })));
    expect(JSON.stringify(vi.mocked(adapter.downloadFile).mock.calls)).not.toContain("C:\\");
  });
});

function port(overrides: Partial<RemoteFilePort> = {}): RemoteFilePort {
  const mutation = async () => ({ status: "completed" as const, result: { committed: true as const, refreshedListing: remoteDirectoryFixture(), message: null } });
  return {
    listDirectory: vi.fn(async ({ path }) => ({ status: "completed" as const, listing: remoteDirectoryFixture(path ? { path, parentPath: "/srv/app" } : {}) })),
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
