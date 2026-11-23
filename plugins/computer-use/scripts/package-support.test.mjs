import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  assertMacPackagingHost,
  assertWindowsPackagingHost,
  readDeclaredVersion,
  windowsPlatformFromPortableExecutable,
} from "./package-support.mjs";

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

test("platform packages require only their native host", () => {
  assertMacPackagingHost("darwin");
  assert.throws(
    () => assertMacPackagingHost("win32"),
    /macOS Computer Use package requires macOS.*received win32/s,
  );
  assertWindowsPackagingHost("win32");
  assert.throws(
    () => assertWindowsPackagingHost("darwin"),
    /Windows Computer Use package requires Windows.*received darwin/s,
  );
});

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

test("keeps separate platform packages on the shared native version", () => {
  const macPackage = JSON.parse(
    readFileSync(path.join(projectRoot, "package.json"), "utf8"),
  );
  const macManifest = JSON.parse(
    readFileSync(path.join(projectRoot, "chatos.plugin.json"), "utf8"),
  );
  const windowsPackage = JSON.parse(
    readFileSync(path.join(projectRoot, "windows", "package.json"), "utf8"),
  );
  const windowsManifest = JSON.parse(
    readFileSync(path.join(projectRoot, "windows", "chatos.plugin.json"), "utf8"),
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

  assert.equal(macPackage.name, "open-computer-use");
  assert.equal(windowsPackage.name, "open-computer-use-windows");
  assert.equal(macManifest.name, macPackage.name);
  assert.equal(windowsManifest.name, windowsPackage.name);
  assert.equal(macManifest.version, macPackage.version);
  assert.equal(windowsManifest.version, windowsPackage.version);
  assert.equal(windowsPackage.version, macPackage.version);
  assert.equal(readDeclaredVersion(swiftService, "Swift service"), macPackage.version);
  assert.equal(readDeclaredVersion(windowsProject, "Windows project"), windowsPackage.version);
  assert.equal(readDeclaredVersion(windowsService, "Windows service"), windowsPackage.version);
  assert.deepEqual(macManifest.dependencies.supportedPlatforms, ["macos-arm64"]);
  assert.deepEqual(windowsManifest.dependencies.supportedPlatforms, [
    "windows-x64",
    "windows-arm64",
  ]);
});

test("routes skill-gate audits through the native worker for each supported host", () => {
  const auditScript = readFileSync(
    path.join(projectRoot, "scripts", "audit-skill-gates.mjs"),
    "utf8",
  );
  assert.match(auditScript, /process\.platform === "win32"/);
  assert.match(auditScript, /path\.join\(packageRoot, "bin", "open-computer-use"\)/);
  assert.match(auditScript, /process\.platform === "darwin"/);
  assert.match(auditScript, /swift.*visual-computer-use-mcp/s);
});
