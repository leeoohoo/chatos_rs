import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RemoteTerminalPoll } from "../domain/remoteTerminal";
import type { RemoteTerminalPort } from "../ports/RemoteTerminalPort";
import { useRemoteTerminalStore } from "./remoteTerminalStore";

describe("remote terminal store", () => {
  beforeEach(resetStore);

  it("resumes MFA in the same presentation and starts one poll loop", async () => {
    const pendingPoll = deferred<RemoteTerminalPoll>();
    const start = vi.fn()
      .mockResolvedValueOnce({ status: "verification_required", prompt: "OTP code:" })
      .mockResolvedValueOnce({ status: "started", sessionId: "remote-terminal-1", initialWorkingDirectory: "/srv/app" });
    const adapter = port({ start, poll: vi.fn(() => pendingPoll.promise) });
    await activate(adapter);
    const presentationId = start.mock.calls[0][0].presentationId;
    useRemoteTerminalStore.getState().setVerificationCode(" 123456 ");
    await useRemoteTerminalStore.getState().submitVerification(adapter);
    expect(start.mock.calls[1][0]).toMatchObject({ presentationId, verificationCode: "123456" });
    expect(useRemoteTerminalStore.getState()).toMatchObject({ phase: "running", workingDirectory: "/srv/app", sessionId: "remote-terminal-1" });
    expect(adapter.poll).toHaveBeenCalledOnce();
  });

  it("projects ordered output, closes a terminated session, and rejects further input", async () => {
    const adapter = port({
      poll: vi.fn(async () => ({
        sessionId: "remote-terminal-1",
        running: false,
        exitCode: 7,
        events: [
          { kind: "output" as const, cursor: 1, data: new TextEncoder().encode("done\r\n") },
          { kind: "terminated" as const, cursor: 2, exitCode: 7 },
        ],
        nextCursor: 2,
        truncated: false,
        timedOut: false,
      })),
    });
    await activate(adapter);
    await vi.waitFor(() => expect(useRemoteTerminalStore.getState().phase).toBe("exited"));
    expect(useRemoteTerminalStore.getState().events.map((event) => event.cursor)).toEqual([1, 2]);
    expect(useRemoteTerminalStore.getState().exitCode).toBe(7);
    expect(adapter.close).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "remote-terminal-1" }));
    useRemoteTerminalStore.getState().sendInput(adapter, new TextEncoder().encode("ignored"));
    expect(adapter.sendInput).not.toHaveBeenCalled();
  });

  it("serializes input writes and never reorders terminal bytes", async () => {
    const pendingPoll = deferred<RemoteTerminalPoll>();
    const first = deferred<void>();
    const sendInput = vi.fn().mockImplementationOnce(() => first.promise).mockResolvedValueOnce(undefined);
    const adapter = port({ poll: vi.fn(() => pendingPoll.promise), sendInput });
    await activate(adapter);
    useRemoteTerminalStore.getState().sendInput(adapter, Uint8Array.of(1));
    useRemoteTerminalStore.getState().sendInput(adapter, Uint8Array.of(2));
    await vi.waitFor(() => expect(sendInput).toHaveBeenCalledTimes(1));
    first.resolve();
    await vi.waitFor(() => expect(sendInput).toHaveBeenCalledTimes(2));
    expect([...sendInput.mock.calls[0][0].data]).toEqual([1]);
    expect([...sendInput.mock.calls[1][0].data]).toEqual([2]);
  });

  it("invalidates a late start and discards the closed presentation", async () => {
    const late = deferred<Awaited<ReturnType<RemoteTerminalPort["start"]>>>();
    const adapter = port({ start: vi.fn(() => late.promise) });
    const activating = activate(adapter);
    await vi.waitFor(() => expect(adapter.start).toHaveBeenCalledOnce());
    const presentationId = useRemoteTerminalStore.getState().presentationId;
    useRemoteTerminalStore.getState().deactivate(adapter);
    late.resolve({ status: "started", sessionId: "remote-terminal-late", initialWorkingDirectory: "~" });
    await activating;
    expect(useRemoteTerminalStore.getState().sessionId).toBeNull();
    await vi.waitFor(() => expect(adapter.discardPresentation).toHaveBeenCalledWith({ ownerUserId: "alice", presentationId }));
  });
});

function activate(adapter: RemoteTerminalPort) {
  return useRemoteTerminalStore.getState().activate(adapter, "alice", "remote-1", "Production", "~");
}

function resetStore() {
  useRemoteTerminalStore.setState({
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
  });
}

function port(overrides: Partial<RemoteTerminalPort> = {}): RemoteTerminalPort {
  const pending = new Promise<RemoteTerminalPoll>(() => undefined);
  return {
    start: vi.fn(async () => ({ status: "started" as const, sessionId: "remote-terminal-1", initialWorkingDirectory: "~" })),
    sendInput: vi.fn(async () => undefined),
    resize: vi.fn(async () => undefined),
    interrupt: vi.fn(async () => undefined),
    poll: vi.fn(() => pending),
    close: vi.fn(async () => undefined),
    discardPresentation: vi.fn(async () => undefined),
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
