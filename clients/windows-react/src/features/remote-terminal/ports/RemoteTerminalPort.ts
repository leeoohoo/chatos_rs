import type { RemoteTerminalPoll, RemoteTerminalStart } from "../domain/remoteTerminal";

interface PresentationScope {
  ownerUserId: string;
  presentationId: string;
}

interface SessionScope extends PresentationScope {
  sessionId: string;
}

export interface RemoteTerminalPort {
  start(request: PresentationScope & {
    connectionId: string;
    verificationCode: string | null;
    columns: number;
    rows: number;
  }): Promise<RemoteTerminalStart>;
  sendInput(request: SessionScope & { data: Uint8Array }): Promise<void>;
  resize(request: SessionScope & { columns: number; rows: number }): Promise<void>;
  interrupt(request: SessionScope): Promise<void>;
  poll(request: SessionScope & { afterCursor: number; timeoutMs: number }): Promise<RemoteTerminalPoll>;
  close(request: SessionScope): Promise<void>;
  discardPresentation(request: PresentationScope): Promise<void>;
}
