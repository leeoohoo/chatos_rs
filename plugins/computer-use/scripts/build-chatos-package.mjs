#!/usr/bin/env node

import { chmodSync, mkdirSync, readFileSync, rmSync } from "node:fs";
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
const packageMetadata = JSON.parse(
  readFileSync(path.join(projectRoot, "package.json"), "utf8"),
);
const manifest = JSON.parse(
  readFileSync(path.join(projectRoot, "chatos.plugin.json"), "utf8"),
);

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: projectRoot,
    encoding: "utf8",
    stdio: options.capture ? "pipe" : "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    if (options.capture && result.stderr) process.stderr.write(result.stderr);
    process.exit(result.status ?? 1);
  }
  return options.capture ? result.stdout.trim() : "";
}

if (packageMetadata.name !== manifest.name) {
  throw new Error("package.json and chatos.plugin.json names must match.");
}
if (packageMetadata.version !== manifest.version) {
  throw new Error("package.json and chatos.plugin.json versions must match.");
}
if (
  process.env.GITHUB_REF_TYPE === "tag" &&
  process.env.GITHUB_REF_NAME !== `computer-use-v${packageMetadata.version}`
) {
  throw new Error(
    `Release tag must be computer-use-v${packageMetadata.version}, received ${process.env.GITHUB_REF_NAME}.`,
  );
}

const argumentsSet = new Set(process.argv.slice(2));
if ([...argumentsSet].some(argument => argument !== "--skip-build")) {
  throw new Error(`Unknown packaging argument: ${[...argumentsSet].join(" ")}`);
}
if (!argumentsSet.has("--skip-build")) {
  run("./scripts/build-macos-app.sh", []);
  run(process.execPath, ["./scripts/build-windows-worker.mjs"]);
}

// A locally built single-architecture app must not advertise every Mac.
const architectures = run(
  "/usr/bin/lipo",
  ["-archs", "dist/Visual Computer Use.app/Contents/MacOS/visual-computer-use-mcp"],
  { capture: true },
).split(/\s+/);
const binaryPlatforms = architectures.map(architecture => {
  if (architecture === "arm64") return "macos-arm64";
  if (architecture === "x86_64") return "macos-x64";
  throw new Error(`Unsupported app architecture: ${architecture}`);
});
for (const { directory } of Object.values(supportedWindowsRuntimes)) {
  const executable = path.join(
    projectRoot,
    "bin",
    directory,
    "open-computer-use.exe",
  );
  const platform = windowsPlatformFromPortableExecutable(readFileSync(executable));
  if (platform !== directory) {
    throw new Error(`${executable} contains ${platform} instead of ${directory}.`);
  }
  binaryPlatforms.push(platform);
}
binaryPlatforms.sort();
const declaredPlatforms = [...(manifest.dependencies?.supportedPlatforms ?? [])].sort();
if (JSON.stringify(binaryPlatforms) !== JSON.stringify(declaredPlatforms)) {
  throw new Error("Manifest supportedPlatforms must match the packaged app architectures.");
}

const appVersion = run(
  "/usr/libexec/PlistBuddy",
  [
    "-c",
    "Print :CFBundleShortVersionString",
    "dist/Visual Computer Use.app/Contents/Info.plist",
  ],
  { capture: true },
);
if (appVersion !== packageMetadata.version) {
  throw new Error(
    `App version ${appVersion} does not match package version ${packageMetadata.version}.`,
  );
}

const mcpService = readFileSync(
  path.join(projectRoot, "Sources", "VisualComputerUseMCP", "MCPService.swift"),
  "utf8",
);
if (!mcpService.includes(`version: "${packageMetadata.version}"`)) {
  throw new Error("MCP server version does not match package.json.");
}
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
for (const [source, label] of [
  [windowsProject, "Windows project"],
  [windowsService, "Windows MCP service"],
]) {
  const version = readDeclaredVersion(source, label);
  if (version !== packageMetadata.version) {
    throw new Error(
      `${label} version ${version} does not match package version ${packageMetadata.version}.`,
    );
  }
}

chmodSync(path.join(projectRoot, "bin", "open-computer-use"), 0o755);
const artifactDirectory = path.join(projectRoot, "dist", "chatos-artifacts");
mkdirSync(artifactDirectory, { recursive: true });
const artifactName = `${packageMetadata.name}-${packageMetadata.version}.tgz`;
rmSync(path.join(artifactDirectory, artifactName), { force: true });
run("npm", ["pack", "--pack-destination", artifactDirectory]);
console.log(`ChatOS artifact: ${path.join(artifactDirectory, artifactName)}`);
