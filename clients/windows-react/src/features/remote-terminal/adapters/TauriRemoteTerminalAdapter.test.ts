import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriRemoteTerminalAdapter } from "./TauriRemoteTerminalAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriRemoteTerminalAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("encodes input bytes before crossing the Tauri boundary", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await new TauriRemoteTerminalAdapter().sendInput({
      ownerUserId: "alice",
      presentationId: "terminal-view-1",
      sessionId: "remote-terminal-1",
      data: Uint8Array.from([0, 255, 128]),
    });
    expect(invoke).toHaveBeenCalledWith("send_remote_terminal_input", {
      ownerUserId: "alice",
      presentationId: "terminal-view-1",
      sessionId: "remote-terminal-1",
      dataBase64: "AP+A",
    });
  });

  it("decodes native poll output and preserves termination events", async () => {
    vi.mocked(invoke).mockResolvedValue({
      sessionId: "remote-terminal-1",
      running: false,
      exitCode: 0,
      events: [
        { kind: "output", cursor: 1, dataBase64: "b2sNCg==" },
        { kind: "terminated", cursor: 2, exitCode: 0 },
      ],
      nextCursor: 2,
      truncated: false,
      timedOut: false,
    });
    const result = await new TauriRemoteTerminalAdapter().poll({
      ownerUserId: "alice",
      presentationId: "terminal-view-1",
      sessionId: "remote-terminal-1",
      afterCursor: 0,
      timeoutMs: 30_000,
    });
    expect(result.events[0]).toMatchObject({ kind: "output", cursor: 1 });
    expect([...((result.events[0] as { data: Uint8Array }).data)]).toEqual([...new TextEncoder().encode("ok\r\n")]);
    expect(result.events[1]).toEqual({ kind: "terminated", cursor: 2, exitCode: 0 });
  });
});
