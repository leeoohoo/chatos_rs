import type { LocalTerminalPoll, LocalTerminalStart } from "../domain/localTerminal";

export interface LocalTerminalPresentationScope {
  ownerUserId: string;
  presentationId: string;
}

export interface LocalTerminalSessionScope extends LocalTerminalPresentationScope {
  sessionId: string;
}

export interface LocalTerminalPort {
  start(request: LocalTerminalPresentationScope & {
    columns: number;
    rows: number;
  }): Promise<LocalTerminalStart>;
  sendInput(request: LocalTerminalSessionScope & { data: Uint8Array }): Promise<void>;
  resize(request: LocalTerminalSessionScope & { columns: number; rows: number }): Promise<void>;
  interrupt(request: LocalTerminalSessionScope): Promise<void>;
  poll(request: LocalTerminalSessionScope & {
    afterCursor: number;
    timeoutMs: number;
  }): Promise<LocalTerminalPoll>;
  close(request: LocalTerminalSessionScope): Promise<void>;
  discardPresentation(request: LocalTerminalPresentationScope): Promise<void>;
}
