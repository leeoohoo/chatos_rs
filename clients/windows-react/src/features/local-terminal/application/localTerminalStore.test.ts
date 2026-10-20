import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LocalTerminalPoll } from "../domain/localTerminal";
import type { LocalTerminalPort } from "../ports/LocalTerminalPort";
import { useLocalTerminalStore } from "./localTerminalStore";

describe("local terminal store", () => {
  beforeEach(resetStore);

  it("creates independent tabs and immediately replaces the last closed tab", async () => {
    const adapter = port();
    await activate(adapter);
    await vi.waitFor(() => expect(useLocalTerminalStore.getState().tabs).toHaveLength(1));
    useLocalTerminalStore.getState().createTerminal(adapter);
    await vi.waitFor(() => expect(useLocalTerminalStore.getState().tabs).toHaveLength(2));
    const [first, second] = useLocalTerminalStore.getState().tabs;
    expect(first.presentationId).not.toBe(second.presentationId);

    useLocalTerminalStore.getState().closeTerminal(adapter, second.id);
    expect(useLocalTerminalStore.getState().selectedTabId).toBe(first.id);
    useLocalTerminalStore.getState().closeTerminal(adapter, first.id);
    await vi.waitFor(() => expect(useLocalTerminalStore.getState().tabs).toHaveLength(1));
    expect(useLocalTerminalStore.getState().tabs[0].id).not.toBe(first.id);
  });

  it("rejects a late native start after the page is deactivated", async () => {
    const late = deferred<Awaited<ReturnType<LocalTerminalPort["start"]>>>();
    const adapter = port({ start: vi.fn(() => late.promise) });
    await activate(adapter);
    await vi.waitFor(() => expect(adapter.start).toHaveBeenCalledOnce());
    const presentationId = useLocalTerminalStore.getState().tabs[0].presentationId;
    useLocalTerminalStore.getState().deactivate(adapter);
    late.resolve({
      sessionId: "local-terminal-late",
      initialWorkingDirectory: "C:\\Users\\Alice",
      shellName: "PowerShell",
    });
    await vi.waitFor(() => expect(adapter.discardPresentation).toHaveBeenCalledWith({
      ownerUserId: "alice",
      presentationId,
    }));
    expect(useLocalTerminalStore.getState().tabs).toEqual([]);
  });

  it("projects ordered output, termination, and closes the native session", async () => {
    const adapter = port({
      poll: vi.fn(async () => ({
        sessionId: "local-terminal-1",
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
    await vi.waitFor(() => expect(useLocalTerminalStore.getState().tabs[0]?.phase).toBe("exited"));
    expect(useLocalTerminalStore.getState().tabs[0].events.map((event) => event.cursor)).toEqual([1, 2]);
    expect(adapter.close).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "local-terminal-1" }));
  });

  it("serializes input separately for each native session", async () => {
    const firstWrite = deferred<void>();
    const sendInput = vi.fn().mockImplementationOnce(() => firstWrite.promise).mockResolvedValueOnce(undefined);
    const adapter = port({ sendInput });
    await activate(adapter);
    await vi.waitFor(() => expect(useLocalTerminalStore.getState().tabs[0]?.phase).toBe("running"));
    const tabId = useLocalTerminalStore.getState().tabs[0].id;
    useLocalTerminalStore.getState().sendInput(adapter, tabId, Uint8Array.of(1));
    useLocalTerminalStore.getState().sendInput(adapter, tabId, Uint8Array.of(2));
    await vi.waitFor(() => expect(sendInput).toHaveBeenCalledTimes(1));
    firstWrite.resolve();
    await vi.waitFor(() => expect(sendInput).toHaveBeenCalledTimes(2));
    expect([...sendInput.mock.calls[0][0].data]).toEqual([1]);
    expect([...sendInput.mock.calls[1][0].data]).toEqual([2]);
  });
});

function activate(adapter: LocalTerminalPort) {
  return useLocalTerminalStore.getState().activate(adapter, "alice", true);
}

function resetStore() {
  useLocalTerminalStore.setState({
    ownerUserId: null,
    hostAvailable: false,
    generation: useLocalTerminalStore.getState().generation + 1,
    tabs: [],
    selectedTabId: null,
  });
}

function port(overrides: Partial<LocalTerminalPort> = {}): LocalTerminalPort {
  const pending = new Promise<LocalTerminalPoll>(() => undefined);
  let session = 0;
  return {
    start: vi.fn(async () => {
      session += 1;
      return {
        sessionId: `local-terminal-${session}`,
        initialWorkingDirectory: "C:\\Users\\Alice",
        shellName: "PowerShell",
      };
    }),
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
