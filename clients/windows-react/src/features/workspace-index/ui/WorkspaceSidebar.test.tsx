import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRemoteConnectionStore } from "../../remote-connection/application/remoteConnectionStore";
import type { RemoteConnectionPort } from "../../remote-connection/ports/RemoteConnectionPort";
import { remoteConnectionFixture } from "../../../test/remoteConnectionFixture";
import { useWorkspaceIndexStore } from "../application/workspaceIndexStore";
import type { WorkspaceIndexPort } from "../ports/WorkspaceIndexPort";
import { WorkspaceSidebar } from "./WorkspaceSidebar";

beforeEach(() => {
  useWorkspaceIndexStore.getState().reset();
  useRemoteConnectionStore.getState().reset();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("WorkspaceSidebar remote connections", () => {
  it("loads the active owner's remote list once and exposes selection and creation", async () => {
    const remote = remotePort();
    const onSelectionChange = vi.fn();
    const onCreateRemoteConnection = vi.fn();
    render(
      <WorkspaceSidebar
        hostReady
        ownerUserId="alice"
        remoteConnection={remote}
        selection={{ kind: "workbench" }}
        workspaceIndex={workspacePort()}
        onCreateProject={() => undefined}
        onCreateRemoteConnection={onCreateRemoteConnection}
        onSelectionChange={onSelectionChange}
      />,
    );
    await screen.findByRole("button", { name: /Production/ });
    expect(remote.list).toHaveBeenCalledOnce();
    expect(screen.getByText("deploy@server.example.com:22")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /Production/ }));
    expect(onSelectionChange).toHaveBeenCalledWith({ kind: "remote", id: "remote-1" });
    fireEvent.click(screen.getByRole("button", { name: "新建远程连接" }));
    expect(onCreateRemoteConnection).toHaveBeenCalledOnce();
    await waitFor(() => expect(remote.list).toHaveBeenCalledTimes(1));
  });
});

function workspacePort(): WorkspaceIndexPort {
  return {
    listLocalProjects: vi.fn(async () => []),
    refreshRelations: vi.fn(async () => ({ contacts: [], conversations: [] })),
  };
}

function remotePort(): RemoteConnectionPort {
  return {
    list: vi.fn(async () => [remoteConnectionFixture()]),
    get: vi.fn(async () => remoteConnectionFixture()),
    create: vi.fn(async () => remoteConnectionFixture()),
    update: vi.fn(async () => remoteConnectionFixture({ version: 2 })),
    delete: vi.fn(async () => "remote-1"),
    testDraft: vi.fn(async () => ({ success: true, verificationRequired: false, message: "连接测试成功。", verificationPrompt: null })),
    testSaved: vi.fn(async () => ({ success: true, verificationRequired: false, message: "连接测试成功。", verificationPrompt: null })),
    pickCredentialFile: vi.fn(async () => null),
    discardCredentialFileSelections: vi.fn(async () => undefined),
  };
}
