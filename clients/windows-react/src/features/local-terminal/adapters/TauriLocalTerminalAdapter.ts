import { invoke } from "@tauri-apps/api/core";
import {
  decodeLocalTerminalPoll,
  encodeLocalTerminalInput,
  localTerminalDimensionSchema,
  localTerminalStartSchema,
  localTerminalWirePollSchema,
} from "../domain/localTerminal";
import type { LocalTerminalPort } from "../ports/LocalTerminalPort";

export class TauriLocalTerminalAdapter implements LocalTerminalPort {
  async start(request: Parameters<LocalTerminalPort["start"]>[0]) {
    return localTerminalStartSchema.parse(await invoke("start_local_terminal", {
      ...request,
      columns: localTerminalDimensionSchema.parse(request.columns),
      rows: localTerminalDimensionSchema.parse(request.rows),
    }));
  }

  async sendInput(request: Parameters<LocalTerminalPort["sendInput"]>[0]) {
    await invoke("send_local_terminal_input", {
      ownerUserId: request.ownerUserId,
      presentationId: request.presentationId,
      sessionId: request.sessionId,
      dataBase64: encodeLocalTerminalInput(request.data),
    });
  }

  async resize(request: Parameters<LocalTerminalPort["resize"]>[0]) {
    await invoke("resize_local_terminal", {
      ...request,
      columns: localTerminalDimensionSchema.parse(request.columns),
      rows: localTerminalDimensionSchema.parse(request.rows),
    });
  }

  async interrupt(request: Parameters<LocalTerminalPort["interrupt"]>[0]) {
    await invoke("interrupt_local_terminal", { ...request });
  }

  async poll(request: Parameters<LocalTerminalPort["poll"]>[0]) {
    const wire = localTerminalWirePollSchema.parse(
      await invoke("poll_local_terminal", { ...request }),
    );
    return decodeLocalTerminalPoll(wire);
  }

  async close(request: Parameters<LocalTerminalPort["close"]>[0]) {
    await invoke("close_local_terminal", { ...request });
  }

  async discardPresentation(
    request: Parameters<LocalTerminalPort["discardPresentation"]>[0],
  ) {
    await invoke("discard_local_terminal_presentation", { ...request });
  }
}
