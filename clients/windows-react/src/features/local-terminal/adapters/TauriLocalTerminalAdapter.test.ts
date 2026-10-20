import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriLocalTerminalAdapter } from "./TauriLocalTerminalAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriLocalTerminalAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("never accepts a shell, cwd, arguments, or environment from React", async () => {
    vi.mocked(invoke).mockResolvedValue({
      sessionId: "local-terminal-1",
      initialWorkingDirectory: "C:\\Users\\Alice",
      shellName: "PowerShell",
    });
    await new TauriLocalTerminalAdapter().start({
      ownerUserId: "alice",
      presentationId: "local-terminal-tab-1",
      columns: 80,
      rows: 24,
    });
    expect(invoke).toHaveBeenCalledWith("start_local_terminal", {
      ownerUserId: "alice",
      presentationId: "local-terminal-tab-1",
      columns: 80,
      rows: 24,
    });
  });

  it("encodes raw input bytes before crossing the Tauri boundary", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await new TauriLocalTerminalAdapter().sendInput({
      ownerUserId: "alice",
      presentationId: "local-terminal-tab-1",
      sessionId: "local-terminal-1",
      data: Uint8Array.from([0, 255, 128]),
    });
    expect(invoke).toHaveBeenCalledWith("send_local_terminal_input", {
      ownerUserId: "alice",
      presentationId: "local-terminal-tab-1",
      sessionId: "local-terminal-1",
      dataBase64: "AP+A",
    });
  });
});
