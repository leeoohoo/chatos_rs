import { describe, expect, it } from "vitest";
import { join } from "node:path";
import {
  buildCargoArguments,
  resolveBuildConfiguration,
  resolveBuiltExecutable,
} from "./prepare-sidecar.mjs";

describe("prepare-sidecar", () => {
  it("uses the Tauri Windows target instead of the build host", () => {
    const configuration = resolveBuildConfiguration(
      {
        TAURI_ENV_DEBUG: "true",
        TAURI_ENV_PLATFORM: "windows",
        TAURI_ENV_TARGET_TRIPLE: "x86_64-pc-windows-gnu",
      },
      "darwin",
    );

    expect(configuration).toEqual({
      targetTriple: "x86_64-pc-windows-gnu",
      profile: "debug",
      executableName: "chatos_local_agent_host.exe",
    });
    expect(buildCargoArguments(configuration)).toContain("x86_64-pc-windows-gnu");
    expect(resolveBuiltExecutable("target", configuration)).toBe(join(
      "target",
      "x86_64-pc-windows-gnu",
      "debug",
      "chatos_local_agent_host.exe",
    ));
  });

  it("keeps native release builds in Cargo's host profile directory", () => {
    const configuration = resolveBuildConfiguration({}, "darwin");

    expect(configuration).toEqual({
      targetTriple: null,
      profile: "release",
      executableName: "chatos_local_agent_host",
    });
    expect(buildCargoArguments(configuration).at(-1)).toBe("--release");
    expect(resolveBuiltExecutable("target", configuration)).toBe(join(
      "target",
      "release",
      "chatos_local_agent_host",
    ));
  });

  it("rejects an unsafe target triple before invoking Cargo", () => {
    expect(() => resolveBuildConfiguration({
      TAURI_ENV_TARGET_TRIPLE: "x86_64-pc-windows-gnu; unwanted",
    })).toThrow("Invalid TAURI_ENV_TARGET_TRIPLE");
  });
});
