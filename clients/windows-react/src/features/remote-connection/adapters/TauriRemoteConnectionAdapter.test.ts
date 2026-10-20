import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { remoteConnectionFixture } from "../../../test/remoteConnectionFixture";
import { keepRemoteCredentials, type RemoteConnectionInput } from "../domain/remoteConnection";
import { TauriRemoteConnectionAdapter } from "./TauriRemoteConnectionAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriRemoteConnectionAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("forwards the editor presentation identity when creating a connection", async () => {
    vi.mocked(invoke).mockResolvedValue(remoteConnectionFixture());
    const input: RemoteConnectionInput = {
      name: "Production",
      host: "server.example.com",
      port: 22,
      username: "deploy",
      authenticationType: "private_key",
      defaultRemotePath: "/srv/app",
      hostKeyPolicy: "strict",
      jumpEnabled: false,
      jumpConnectionId: null,
      jumpHost: null,
      jumpPort: null,
      jumpUsername: null,
      credentials: keepRemoteCredentials(),
    };
    await new TauriRemoteConnectionAdapter().create({
      ownerUserId: "alice",
      presentationId: "remote-connection-editor-1",
      input,
    });
    expect(invoke).toHaveBeenCalledWith("create_remote_connection", {
      ownerUserId: "alice",
      presentationId: "remote-connection-editor-1",
      input,
    });
  });
});
