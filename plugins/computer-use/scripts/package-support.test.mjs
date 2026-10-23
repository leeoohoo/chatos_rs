import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  readDeclaredVersion,
  windowsPlatformFromPortableExecutable,
} from "./package-support.mjs";

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function portableExecutable(machine) {
  const bytes = Buffer.alloc(128);
  bytes.write("MZ", 0, "ascii");
  bytes.writeUInt32LE(64, 0x3c);
  bytes.write("PE\0\0", 64, "binary");
  bytes.writeUInt16LE(machine, 68);
  return bytes;
}

test("maps only supported Windows PE architectures", () => {
  assert.equal(
    windowsPlatformFromPortableExecutable(portableExecutable(0x8664)),
    "windows-x64",
  );
  assert.equal(
    windowsPlatformFromPortableExecutable(portableExecutable(0xaa64)),
    "windows-arm64",
  );
  assert.throws(() => windowsPlatformFromPortableExecutable(Buffer.alloc(64)));
  assert.throws(() =>
    windowsPlatformFromPortableExecutable(portableExecutable(0x014c)),
  );
});

test("reads the shared release version from project sources", () => {
  assert.equal(readDeclaredVersion("<Version>0.9.0</Version>", "project"), "0.9.0");
  assert.equal(readDeclaredVersion('version: "0.9.0"', "service"), "0.9.0");
  assert.equal(readDeclaredVersion('const string Version = "0.9.0";', "service"), "0.9.0");
  assert.throws(() => readDeclaredVersion("Version=latest", "invalid"));
});

test("keeps package, manifest, and both native implementations on one release", () => {
  const packageMetadata = JSON.parse(
    readFileSync(path.join(projectRoot, "package.json"), "utf8"),
  );
  const manifest = JSON.parse(
    readFileSync(path.join(projectRoot, "chatos.plugin.json"), "utf8"),
  );
  const swiftService = readFileSync(
    path.join(projectRoot, "Sources", "VisualComputerUseMCP", "MCPService.swift"),
    "utf8",
  );
  const windowsProject = readFileSync(
    path.join(
      projectRoot,
      "windows",
      "VisualComputerUse.Windows",
      "VisualComputerUse.Windows.csproj",
    ),
    "utf8",
  );
  const windowsService = readFileSync(
    path.join(
      projectRoot,
      "windows",
      "VisualComputerUse.Windows",
      "McpService.cs",
    ),
    "utf8",
  );

  assert.equal(manifest.version, packageMetadata.version);
  assert.equal(readDeclaredVersion(swiftService, "Swift service"), packageMetadata.version);
  assert.equal(readDeclaredVersion(windowsProject, "Windows project"), packageMetadata.version);
  assert.equal(readDeclaredVersion(windowsService, "Windows service"), packageMetadata.version);
  assert.deepEqual(manifest.dependencies.supportedPlatforms, [
    "macos-arm64",
    "windows-x64",
    "windows-arm64",
  ]);
});
