import { invoke } from "@tauri-apps/api/core";
import {
  decodeRemoteTerminalPoll,
  encodeTerminalInput,
  remoteTerminalStartSchema,
  remoteTerminalWirePollSchema,
  terminalDimensionSchema,
} from "../domain/remoteTerminal";
import type { RemoteTerminalPort } from "../ports/RemoteTerminalPort";

export class TauriRemoteTerminalAdapter implements RemoteTerminalPort {
  async start(request: Parameters<RemoteTerminalPort["start"]>[0]) {
    return remoteTerminalStartSchema.parse(await invoke("start_remote_terminal", {
      ...request,
      columns: terminalDimensionSchema.parse(request.columns),
      rows: terminalDimensionSchema.parse(request.rows),
    }));
  }

  async sendInput(request: Parameters<RemoteTerminalPort["sendInput"]>[0]) {
    await invoke("send_remote_terminal_input", {
      ownerUserId: request.ownerUserId,
      presentationId: request.presentationId,
      sessionId: request.sessionId,
      dataBase64: encodeTerminalInput(request.data),
    });
  }

  async resize(request: Parameters<RemoteTerminalPort["resize"]>[0]) {
    await invoke("resize_remote_terminal", {
      ...request,
      columns: terminalDimensionSchema.parse(request.columns),
      rows: terminalDimensionSchema.parse(request.rows),
    });
  }

  async interrupt(request: Parameters<RemoteTerminalPort["interrupt"]>[0]) {
    await invoke("interrupt_remote_terminal", { ...request });
  }

  async poll(request: Parameters<RemoteTerminalPort["poll"]>[0]) {
    const wire = remoteTerminalWirePollSchema.parse(
      await invoke("poll_remote_terminal", { ...request }),
    );
    return decodeRemoteTerminalPoll(wire);
  }

  async close(request: Parameters<RemoteTerminalPort["close"]>[0]) {
    await invoke("close_remote_terminal", { ...request });
  }

  async discardPresentation(
    request: Parameters<RemoteTerminalPort["discardPresentation"]>[0],
  ) {
    await invoke("discard_remote_terminal_presentation", { ...request });
  }
}
