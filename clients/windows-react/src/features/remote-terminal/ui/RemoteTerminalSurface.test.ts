import { describe, expect, it } from "vitest";
import { workingDirectoryFromOsc } from "../domain/remoteTerminal";

describe("RemoteTerminalSurface", () => {
  it("projects OSC 7 file URLs into the same working-directory presentation as macOS", () => {
    expect(workingDirectoryFromOsc("file://server/srv/my%20app")).toBe("/srv/my app");
    expect(workingDirectoryFromOsc("/var/log")).toBe("/var/log");
    expect(workingDirectoryFromOsc("not-a-directory")).toBeNull();
  });
});
