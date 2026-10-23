#!/usr/bin/env node

import {
  copyFileSync,
  mkdirSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  readDeclaredVersion,
  supportedWindowsRuntimes,
  windowsPlatformFromPortableExecutable,
} from "./package-support.mjs";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptDirectory, "..");
const windowsProject = path.join(
  projectRoot,
  "windows",
  "VisualComputerUse.Windows",
  "VisualComputerUse.Windows.csproj",
);
const packageMetadata = JSON.parse(
  readFileSync(path.join(projectRoot, "package.json"), "utf8"),
);

const requestedRuntimes = [];
for (let index = 2; index < process.argv.length; index += 1) {
  if (process.argv[index] !== "--runtime" || !process.argv[index + 1]) {
    throw new Error(`Unknown argument: ${process.argv[index]}`);
  }
  requestedRuntimes.push(process.argv[index + 1]);
  index += 1;
}
const runtimes = requestedRuntimes.length
  ? [...new Set(requestedRuntimes)]
  : Object.keys(supportedWindowsRuntimes);

const projectSource = readFileSync(windowsProject, "utf8");
const serviceSource = readFileSync(
  path.join(
    projectRoot,
    "windows",
    "VisualComputerUse.Windows",
    "McpService.cs",
  ),
  "utf8",
);
for (const [source, label] of [
  [projectSource, "Windows project"],
  [serviceSource, "Windows MCP service"],
]) {
  const version = readDeclaredVersion(source, label);
  if (version !== packageMetadata.version) {
    throw new Error(
      `${label} version ${version} does not match package version ${packageMetadata.version}.`,
    );
  }
}

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: projectRoot,
    encoding: "utf8",
    stdio: "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

for (const runtime of runtimes) {
  const expected = supportedWindowsRuntimes[runtime];
  if (!expected) throw new Error(`Unsupported Windows runtime: ${runtime}.`);
  const publishDirectory = path.join(projectRoot, "dist", "windows", runtime);
  rmSync(publishDirectory, { recursive: true, force: true });
  mkdirSync(publishDirectory, { recursive: true });
  run("dotnet", [
    "publish",
    windowsProject,
    "--configuration",
    "Release",
    "--runtime",
    runtime,
    "--self-contained",
    "true",
    "--output",
    publishDirectory,
    "-p:PublishSingleFile=true",
    "-p:IncludeNativeLibrariesForSelfExtract=true",
    "-p:EnableCompressionInSingleFile=true",
    "-p:PublishTrimmed=false",
    "-p:DebugSymbols=false",
    "-p:DebugType=None",
  ]);

  const source = path.join(publishDirectory, "visual-computer-use-mcp.exe");
  const platform = windowsPlatformFromPortableExecutable(readFileSync(source));
  if (platform !== expected.directory) {
    throw new Error(
      `Published ${runtime} worker contains ${platform} instead of ${expected.directory}.`,
    );
  }
  const destinationDirectory = path.join(projectRoot, "bin", expected.directory);
  mkdirSync(destinationDirectory, { recursive: true });
  const destination = path.join(destinationDirectory, "open-computer-use.exe");
  rmSync(destination, { force: true });
  copyFileSync(source, destination);
  console.log(`Windows worker: ${destination}`);
}
