import { describe, expect, it } from "vitest";
import { remoteConnectionFixture } from "../../../test/remoteConnectionFixture";
import {
  buildRemoteConnectionInput,
  initialRemoteConnectionDraft,
} from "./remoteConnectionDraft";

describe("remote connection draft", () => {
  it("reuses saved credentials when editing without exposing their values", () => {
    const editing = remoteConnectionFixture();
    const draft = initialRemoteConnectionDraft(editing, [editing]);
    const input = buildRemoteConnectionInput(draft, editing, [editing]);
    expect(input.credentials.privateKeyPath).toEqual({ action: "keep" });
    expect(input.credentials.password).toEqual({ action: "clear" });
    expect(JSON.stringify(input)).not.toContain("C:\\");
    expect(JSON.stringify(input)).not.toContain("/Users/");
  });

  it("requires a native opaque selection for a new private-key connection", () => {
    const draft = {
      ...initialRemoteConnectionDraft(null, []),
      host: "server.example.com",
      username: "deploy",
    };
    expect(() => buildRemoteConnectionInput(draft, null, [])).toThrow("请选择私钥文件。");
    draft.privateKeySelection = {
      selectionId: "remote-credential-file-key-1",
      fileName: "id_ed25519",
      kind: "private_key",
    };
    expect(buildRemoteConnectionInput(draft, null, []).credentials.privateKeyPath).toEqual({
      action: "replace",
      value: "remote-credential-file-key-1",
    });
  });

  it("submits only the existing jump identity and lets Rust resolve its current route", () => {
    const jump = remoteConnectionFixture({ id: "jump-1", name: "Bastion" });
    const draft = {
      ...initialRemoteConnectionDraft(null, [jump]),
      host: "server.example.com",
      username: "deploy",
      privateKeySelection: {
        selectionId: "remote-credential-file-key-1",
        fileName: "id_ed25519",
        kind: "private_key" as const,
      },
      jumpEnabled: true,
      jumpMode: "existing" as const,
      jumpConnectionId: "jump-1",
    };
    const input = buildRemoteConnectionInput(draft, null, [jump]);
    expect(input).toMatchObject({
      jumpConnectionId: "jump-1",
      jumpHost: null,
      jumpPort: null,
      jumpUsername: null,
    });
  });
});
