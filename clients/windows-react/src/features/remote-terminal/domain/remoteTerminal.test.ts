import { describe, expect, it } from "vitest";
import { decodeRemoteTerminalPoll, encodeTerminalInput, remoteTerminalWirePollSchema } from "./remoteTerminal";

describe("remote terminal domain", () => {
  it("decodes bounded byte output without treating it as text", () => {
    const wire = remoteTerminalWirePollSchema.parse({
      sessionId: "remote-terminal-1",
      running: true,
      exitCode: null,
      events: [{ kind: "output", cursor: 1, dataBase64: "AP+A" }],
      nextCursor: 1,
      truncated: false,
      timedOut: false,
    });
    const event = decodeRemoteTerminalPoll(wire).events[0];
    expect(event.kind).toBe("output");
    if (event.kind !== "output") throw new Error("expected output event");
    expect([...event.data]).toEqual([0, 255, 128]);
  });

  it("encodes UTF-8 terminal input as the native byte contract", () => {
    expect(encodeTerminalInput(new TextEncoder().encode("你好\r"))).toBe("5L2g5aW9DQ==");
  });

  it("rejects unordered or future terminal cursors", () => {
    expect(() => remoteTerminalWirePollSchema.parse({
      sessionId: "remote-terminal-1",
      running: true,
      exitCode: null,
      events: [
        { kind: "output", cursor: 2, dataBase64: "YQ==" },
        { kind: "output", cursor: 1, dataBase64: "Yg==" },
      ],
      nextCursor: 2,
      truncated: false,
      timedOut: false,
    })).toThrow();
  });
});
