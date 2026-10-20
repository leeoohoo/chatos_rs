import { describe, expect, it } from "vitest";
import {
  localTerminalSessionIdSchema,
  localTerminalTitleForDirectory,
  localTerminalWirePollSchema,
  localWorkingDirectoryFromOsc,
} from "./localTerminal";

describe("local terminal domain", () => {
  it("keeps local sessions in a namespace distinct from remote terminals", () => {
    expect(localTerminalSessionIdSchema.parse("local-terminal-1234-abcd")).toBe("local-terminal-1234-abcd");
    expect(() => localTerminalSessionIdSchema.parse("remote-terminal-1234")).toThrow();
  });

  it("rejects unordered or oversized native event projections", () => {
    expect(() => localTerminalWirePollSchema.parse({
      sessionId: "local-terminal-1",
      running: true,
      exitCode: null,
      events: [
        { kind: "terminated", cursor: 2, exitCode: 0 },
        { kind: "terminated", cursor: 1, exitCode: 0 },
      ],
      nextCursor: 2,
      truncated: false,
      timedOut: false,
    })).toThrow("本机终端事件游标无效");
  });

  it("projects macOS-compatible OSC paths and cross-platform tab titles", () => {
    expect(localWorkingDirectoryFromOsc("file:///C:/Users/Alice/Work")).toBe("C:/Users/Alice/Work");
    expect(localWorkingDirectoryFromOsc("C:\\Users\\Alice\\Work")).toBe("C:\\Users\\Alice\\Work");
    expect(localWorkingDirectoryFromOsc("relative/path")).toBeNull();
    expect(localTerminalTitleForDirectory("C:\\Users\\Alice\\Work\\")).toBe("Work");
    expect(localTerminalTitleForDirectory("/Users/alice/project")).toBe("project");
  });
});
