import { create } from "zustand";
import type { RemoteTerminalEvent } from "../domain/remoteTerminal";
import type { RemoteTerminalPort } from "../ports/RemoteTerminalPort";

export type RemoteTerminalPhase = "idle" | "connecting" | "running" | "exited" | "failed" | "closed";
type TerminalAction = "interrupt" | "disconnect" | "cancel_verification";

interface RemoteTerminalState {
  ownerUserId: string | null;
  connectionId: string | null;
  connectionName: string;
  presentationId: string | null;
  sessionId: string | null;
  generation: number;
  connectRequestId: number;
  phase: RemoteTerminalPhase;
  title: string;
  workingDirectory: string;
  columns: number;
  rows: number;
  cursor: number;
  events: RemoteTerminalEvent[];
  retainedOutputBytes: number;
  outputGeneration: number;
  exitCode: number | null;
  activeAction: TerminalAction | null;
  errorMessage: string | null;
  warningMessage: string | null;
  verificationPrompt: string | null;
  verificationCode: string;
  activate(port: RemoteTerminalPort, ownerUserId: string, connectionId: string, connectionName: string, workingDirectory: string): Promise<void>;
  deactivate(port: RemoteTerminalPort): void;
  reconnect(port: RemoteTerminalPort): Promise<void>;
  disconnect(port: RemoteTerminalPort): Promise<void>;
  interrupt(port: RemoteTerminalPort): Promise<void>;
  sendInput(port: RemoteTerminalPort, data: Uint8Array): void;
  resize(port: RemoteTerminalPort, columns: number, rows: number): Promise<void>;
  submitVerification(port: RemoteTerminalPort): Promise<void>;
  cancelVerification(port: RemoteTerminalPort): Promise<void>;
  setVerificationCode(value: string): void;
  updateTitle(value: string): void;
  updateWorkingDirectory(value: string): void;
  clearOutput(): void;
  dismissFeedback(): void;
}

interface SessionContext {
  ownerUserId: string;
  presentationId: string;
  sessionId: string;
  generation: number;
}

const maximumRetainedOutputBytes = 2 * 1_024 * 1_024;
let presentationSequence = 0;
let activationSequence = 0;
let discardBarrier: Promise<void> = Promise.resolve();
let inputBarrier: Promise<void> = Promise.resolve();

export const useRemoteTerminalStore = create<RemoteTerminalState>((set, get) => ({
  ownerUserId: null,
  connectionId: null,
  connectionName: "",
  presentationId: null,
  sessionId: null,
  generation: 0,
  connectRequestId: 0,
  phase: "idle",
  title: "",
  workingDirectory: "~",
  columns: 80,
  rows: 24,
  cursor: 0,
  events: [],
  retainedOutputBytes: 0,
  outputGeneration: 0,
  exitCode: null,
  activeAction: null,
  errorMessage: null,
  warningMessage: null,
  verificationPrompt: null,
  verificationCode: "",

  async activate(port, ownerInput, connectionInput, connectionNameInput, workingDirectoryInput) {
    const ownerUserId = ownerInput.trim();
    const connectionId = connectionInput.trim();
    const connectionName = connectionNameInput.trim();
    if (!ownerUserId || !connectionId || !connectionName) return;
    const activation = ++activationSequence;
    await discardBarrier;
    if (activation !== activationSequence) return;
    const current = get();
    set({
      ...resetState(current),
      ownerUserId,
      connectionId,
      connectionName,
      presentationId: `remote-terminal-view-${Date.now()}-${++presentationSequence}`,
      generation: current.generation + 1,
      phase: "idle",
      title: connectionName,
      workingDirectory: workingDirectoryInput.trim() || "~",
      outputGeneration: current.outputGeneration + 1,
    });
    await connect(port, null, get, set);
  },

  deactivate(port) {
    ++activationSequence;
    const current = get();
    if (current.ownerUserId && current.presentationId) {
      const request = { ownerUserId: current.ownerUserId, presentationId: current.presentationId };
      discardBarrier = discardBarrier.then(() => port.discardPresentation(request)).catch(() => undefined);
    }
    inputBarrier = Promise.resolve();
    set(resetState(current));
  },

  async reconnect(port) {
    const current = get();
    if (!current.ownerUserId || !current.presentationId || !current.connectionId) return;
    const invalidated = current.connectRequestId + 1;
    set({
      connectRequestId: invalidated,
      sessionId: null,
      phase: "idle",
      cursor: 0,
      events: [],
      retainedOutputBytes: 0,
      outputGeneration: current.outputGeneration + 1,
      exitCode: null,
      errorMessage: null,
      warningMessage: null,
      verificationPrompt: null,
      verificationCode: "",
    });
    if (current.sessionId) {
      await port.close(sessionRequest(current)).catch(() => undefined);
    }
    await connect(port, null, get, set);
  },

  async disconnect(port) {
    const current = get();
    if (!current.ownerUserId || !current.presentationId || current.activeAction) return;
    const actionGeneration = current.generation;
    const connectRequestId = current.connectRequestId + 1;
    set({ activeAction: "disconnect", connectRequestId });
    try {
      if (current.sessionId) await port.close(sessionRequest(current));
      else await port.discardPresentation({ ownerUserId: current.ownerUserId, presentationId: current.presentationId });
      if (!acceptsPresentation(get(), current.ownerUserId, current.presentationId, actionGeneration)) return;
      set({ activeAction: null, sessionId: null, phase: "closed", verificationPrompt: null, verificationCode: "" });
    } catch (error) {
      if (!acceptsPresentation(get(), current.ownerUserId, current.presentationId, actionGeneration)) return;
      set({ activeAction: null, errorMessage: errorMessage(error) });
    }
  },

  async interrupt(port) {
    const current = get();
    const context = sessionContext(current);
    if (!context || current.phase !== "running" || current.activeAction) return;
    set({ activeAction: "interrupt", errorMessage: null });
    try {
      await port.interrupt(sessionRequest(current));
      if (acceptsSession(get(), context)) set({ activeAction: null });
    } catch (error) {
      if (acceptsSession(get(), context)) set({ activeAction: null, errorMessage: errorMessage(error) });
    }
  },

  sendInput(port, data) {
    const current = get();
    const context = sessionContext(current);
    if (!context || current.phase !== "running" || data.length === 0 || data.length > 64 * 1_024) return;
    const request = { ...sessionRequest(current), data: new Uint8Array(data) };
    inputBarrier = inputBarrier
      .then(() => port.sendInput(request))
      .catch((error) => {
        if (acceptsSession(get(), context)) set({ errorMessage: errorMessage(error) });
      });
  },

  async resize(port, columnsInput, rowsInput) {
    const columns = boundedDimension(columnsInput);
    const rows = boundedDimension(rowsInput);
    set({ columns, rows });
    const current = get();
    const context = sessionContext(current);
    if (!context || current.phase !== "running") return;
    try {
      await port.resize({ ...sessionRequest(current), columns, rows });
    } catch (error) {
      if (acceptsSession(get(), context)) set({ errorMessage: errorMessage(error) });
    }
  },

  async submitVerification(port) {
    const current = get();
    const code = current.verificationCode.trim();
    if (!code) {
      set({ errorMessage: "请输入验证码。" });
      return;
    }
    if (!current.verificationPrompt || current.phase === "connecting") return;
    await connect(port, code, get, set);
  },

  async cancelVerification(port) {
    const current = get();
    if (!current.ownerUserId || !current.presentationId || !current.verificationPrompt || current.activeAction) return;
    const generation = current.generation;
    set({ activeAction: "cancel_verification", connectRequestId: current.connectRequestId + 1 });
    try {
      await port.discardPresentation({ ownerUserId: current.ownerUserId, presentationId: current.presentationId });
      if (!acceptsPresentation(get(), current.ownerUserId, current.presentationId, generation)) return;
      set({ activeAction: null, phase: "failed", verificationPrompt: null, verificationCode: "", errorMessage: "SSH 二次验证已取消。" });
    } catch (error) {
      if (acceptsPresentation(get(), current.ownerUserId, current.presentationId, generation)) set({ activeAction: null, errorMessage: errorMessage(error) });
    }
  },

  setVerificationCode(value) { if (get().phase !== "connecting") set({ verificationCode: value, errorMessage: null }); },
  updateTitle(value) { const title = value.trim(); if (title && get().presentationId) set({ title }); },
  updateWorkingDirectory(value) { const path = value.trim(); if (path && get().presentationId) set({ workingDirectory: path }); },
  clearOutput() { set((state) => ({ events: [], retainedOutputBytes: 0, outputGeneration: state.outputGeneration + 1 })); },
  dismissFeedback() { set({ errorMessage: null, warningMessage: null }); },
}));

type StoreGet = () => RemoteTerminalState;
type StoreSet = (partial: Partial<RemoteTerminalState> | ((state: RemoteTerminalState) => Partial<RemoteTerminalState>)) => void;

async function connect(port: RemoteTerminalPort, verificationCode: string | null, get: StoreGet, set: StoreSet) {
  const current = get();
  if (!current.ownerUserId || !current.presentationId || !current.connectionId || current.phase === "connecting") return;
  const requestId = current.connectRequestId + 1;
  const generation = current.generation;
  set({ phase: "connecting", connectRequestId: requestId, errorMessage: null, verificationPrompt: null, verificationCode: "" });
  try {
    const result = await port.start({
      ownerUserId: current.ownerUserId,
      presentationId: current.presentationId,
      connectionId: current.connectionId,
      verificationCode,
      columns: current.columns,
      rows: current.rows,
    });
    if (!acceptsConnect(get(), current.ownerUserId, current.presentationId, generation, requestId)) return;
    if (result.status === "verification_required") {
      set({ phase: "idle", verificationPrompt: result.prompt, verificationCode: "", errorMessage: verificationCode == null ? null : "验证码未通过，请重新输入。" });
      return;
    }
    const context: SessionContext = { ownerUserId: current.ownerUserId, presentationId: current.presentationId, sessionId: result.sessionId, generation };
    inputBarrier = Promise.resolve();
    set({ sessionId: result.sessionId, phase: "running", workingDirectory: result.initialWorkingDirectory, cursor: 0, events: [], retainedOutputBytes: 0, outputGeneration: get().outputGeneration + 1, exitCode: null, warningMessage: null });
    void pollLoop(port, context, get, set);
  } catch (error) {
    if (acceptsConnect(get(), current.ownerUserId, current.presentationId, generation, requestId)) set({ phase: "failed", errorMessage: errorMessage(error) });
  }
}

async function pollLoop(port: RemoteTerminalPort, context: SessionContext, get: StoreGet, set: StoreSet) {
  while (acceptsSession(get(), context)) {
    const afterCursor = get().cursor;
    try {
      const poll = await port.poll({ ...contextRequest(context), afterCursor, timeoutMs: 30_000 });
      if (!acceptsSession(get(), context)) return;
      if (poll.sessionId !== context.sessionId || poll.nextCursor < afterCursor) throw new Error("远端终端轮询响应无效。");
      appendEvents(poll.events, poll.nextCursor, poll.truncated, set);
      if (!poll.running) {
        set({ sessionId: null, phase: "exited", exitCode: poll.exitCode });
        await port.close(contextRequest(context)).catch(() => undefined);
        return;
      }
    } catch (error) {
      if (!acceptsSession(get(), context)) return;
      set({ sessionId: null, phase: "failed", errorMessage: errorMessage(error) });
      await port.close(contextRequest(context)).catch(() => undefined);
      return;
    }
  }
}

function appendEvents(events: RemoteTerminalEvent[], cursor: number, truncated: boolean, set: StoreSet) {
  set((state) => {
    const unique = events.filter((event) => event.cursor > state.cursor);
    const combined = [...state.events, ...unique];
    let bytes = state.retainedOutputBytes + unique.reduce((total, event) => total + (event.kind === "output" ? event.data.byteLength : 0), 0);
    while (combined.length > 512 || bytes > maximumRetainedOutputBytes) {
      const removed = combined.shift();
      if (removed?.kind === "output") bytes -= removed.data.byteLength;
    }
    return { events: combined, retainedOutputBytes: bytes, cursor, warningMessage: truncated ? "部分较早的终端输出已被截断。" : state.warningMessage };
  });
}

function sessionRequest(state: Pick<RemoteTerminalState, "ownerUserId" | "presentationId" | "sessionId">) {
  return { ownerUserId: state.ownerUserId!, presentationId: state.presentationId!, sessionId: state.sessionId! };
}

function sessionContext(state: RemoteTerminalState): SessionContext | null {
  if (!state.ownerUserId || !state.presentationId || !state.sessionId) return null;
  return { ownerUserId: state.ownerUserId, presentationId: state.presentationId, sessionId: state.sessionId, generation: state.generation };
}

function contextRequest(context: SessionContext) {
  return { ownerUserId: context.ownerUserId, presentationId: context.presentationId, sessionId: context.sessionId };
}

function acceptsPresentation(state: RemoteTerminalState, ownerUserId: string, presentationId: string, generation: number) {
  return state.ownerUserId === ownerUserId && state.presentationId === presentationId && state.generation === generation;
}

function acceptsConnect(state: RemoteTerminalState, ownerUserId: string, presentationId: string, generation: number, requestId: number) {
  return acceptsPresentation(state, ownerUserId, presentationId, generation) && state.connectRequestId === requestId && state.phase === "connecting";
}

function acceptsSession(state: RemoteTerminalState, context: SessionContext) {
  return acceptsPresentation(state, context.ownerUserId, context.presentationId, context.generation) && state.sessionId === context.sessionId;
}

function boundedDimension(value: number) { return Math.min(1_000, Math.max(1, Math.trunc(value) || 1)); }

function resetState(state: RemoteTerminalState): Partial<RemoteTerminalState> {
  return { ownerUserId: null, connectionId: null, connectionName: "", presentationId: null, sessionId: null, generation: state.generation + 1, connectRequestId: state.connectRequestId + 1, phase: "idle", title: "", workingDirectory: "~", cursor: 0, events: [], retainedOutputBytes: 0, outputGeneration: state.outputGeneration + 1, exitCode: null, activeAction: null, errorMessage: null, warningMessage: null, verificationPrompt: null, verificationCode: "" };
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "远端终端通信失败。";
}
