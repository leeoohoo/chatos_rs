import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RemoteConnectionPort } from "../ports/RemoteConnectionPort";
import { remoteConnectionFixture } from "../../../test/remoteConnectionFixture";
import { RemoteConnectionEditorSheet } from "./RemoteConnectionEditorSheet";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("RemoteConnectionEditorSheet", () => {
  it("creates a password connection through the presentation-scoped port", async () => {
    const adapter = port();
    const onDismiss = vi.fn();
    const onSaved = vi.fn();
    render(
      <RemoteConnectionEditorSheet
        connections={[]}
        editingConnection={null}
        ownerUserId="alice"
        port={adapter}
        onDismiss={onDismiss}
        onSaved={onSaved}
      />,
    );

    fireEvent.change(screen.getByRole("textbox", { name: "主机" }), { target: { value: " host.example.com " } });
    fireEvent.change(screen.getByRole("textbox", { name: "用户名" }), { target: { value: " deploy " } });
    fireEvent.change(screen.getByRole("combobox", { name: "认证方式" }), { target: { value: "password" } });
    fireEvent.change(screen.getByLabelText("登录密码"), { target: { value: "secret" } });
    fireEvent.click(screen.getByRole("button", { name: "创建" }));

    await waitFor(() => expect(adapter.create).toHaveBeenCalledWith(expect.objectContaining({
      ownerUserId: "alice",
      presentationId: expect.stringMatching(/^remote-connection-editor-/),
      input: expect.objectContaining({
        host: "host.example.com",
        username: "deploy",
        authenticationType: "password",
        credentials: expect.objectContaining({ password: { action: "replace", value: "secret" } }),
      }),
    })));
    expect(onSaved).toHaveBeenCalledWith(expect.objectContaining({ id: "remote-1" }));
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it("renders only the opaque picker file name and never a native path", async () => {
    const adapter = port({
      pickCredentialFile: vi.fn(async () => ({
        selectionId: "remote-credential-file-key",
        fileName: "id_ed25519",
        kind: "private_key" as const,
      })),
    });
    const { container } = render(
      <RemoteConnectionEditorSheet
        connections={[]}
        editingConnection={null}
        ownerUserId="alice"
        port={adapter}
        onDismiss={() => undefined}
        onSaved={() => undefined}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "选择…" }));
    await screen.findByText("id_ed25519");
    expect(container.textContent).not.toContain("C:\\Users");
    expect(adapter.pickCredentialFile).toHaveBeenCalledWith(expect.objectContaining({
      ownerUserId: "alice",
      kind: "private_key",
      presentationId: expect.stringMatching(/^remote-connection-editor-/),
    }));
  });
});

function port(overrides: Partial<RemoteConnectionPort> = {}): RemoteConnectionPort {
  return {
    list: vi.fn(async () => []),
    get: vi.fn(async () => null),
    create: vi.fn(async () => remoteConnectionFixture({ authenticationType: "password", hasPassword: true })),
    update: vi.fn(async () => remoteConnectionFixture({ version: 2 })),
    delete: vi.fn(async () => "remote-1"),
    testDraft: vi.fn(async () => ({ success: true, verificationRequired: false, message: "连接测试成功。", verificationPrompt: null })),
    testSaved: vi.fn(async () => ({ success: true, verificationRequired: false, message: "连接测试成功。", verificationPrompt: null })),
    pickCredentialFile: vi.fn(async () => null),
    discardCredentialFileSelections: vi.fn(async () => undefined),
    ...overrides,
  };
}
