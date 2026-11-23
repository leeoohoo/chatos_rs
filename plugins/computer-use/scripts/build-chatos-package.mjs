#!/usr/bin/env node

import {
  chmodSync,
  cpSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  assertMacPackagingHost,
  assertWindowsPackagingHost,
  readDeclaredVersion,
  supportedWindowsRuntimes,
  windowsPlatformFromPortableExecutable,
} from "./package-support.mjs";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptDirectory, "..");
const argumentsList = process.argv.slice(2);
let platform;
let skipBuild = false;
for (let index = 0; index < argumentsList.length; index += 1) {
  const argument = argumentsList[index];
  if (argument === "--platform" && argumentsList[index + 1]) {
    platform = argumentsList[index + 1];
    index += 1;
  } else if (argument === "--skip-build") {
    skipBuild = true;
  } else {
    throw new Error(`Unknown packaging argument: ${argument}`);
  }
}
if (!new Set(["macos", "windows"]).has(platform)) {
  throw new Error("--platform must be macos or windows.");
}

const sourceRoot = platform === "macos" ? projectRoot : path.join(projectRoot, "windows");
const packageMetadata = readJSON(path.join(sourceRoot, "package.json"));
const manifest = readJSON(path.join(sourceRoot, "chatos.plugin.json"));
const expectedPlatforms = platform === "macos"
  ? ["macos-arm64"]
  : ["windows-x64", "windows-arm64"];

function readJSON(file) {
  return JSON.parse(readFileSync(file, "utf8"));
}

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

function assertMetadata() {
  if (packageMetadata.name !== manifest.name) {
    throw new Error(`${platform} package.json and chatos.plugin.json names must match.`);
  }
  if (packageMetadata.version !== manifest.version) {
    throw new Error(`${platform} package.json and chatos.plugin.json versions must match.`);
  }
  if (JSON.stringify(manifest.dependencies?.supportedPlatforms) !== JSON.stringify(expectedPlatforms)) {
    throw new Error(`${platform} Manifest supportedPlatforms must be ${expectedPlatforms.join(", ")}.`);
  }
  if (process.env.GITHUB_REF_TYPE === "tag") {
    const tagPrefix = platform === "macos" ? "computer-use-macos-v" : "computer-use-windows-v";
    if (process.env.GITHUB_REF_NAME !== `${tagPrefix}${packageMetadata.version}`) {
      throw new Error(
        `${platform} release tag must be ${tagPrefix}${packageMetadata.version}, received ${process.env.GITHUB_REF_NAME}.`,
      );
    }
  }
}

function buildAndValidateMac() {
  assertMacPackagingHost();
  if (!skipBuild) run("./scripts/build-macos-app.sh", []);
  const appPath = path.join(projectRoot, "dist", "Visual Computer Use.app");
  const executable = path.join(appPath, "Contents", "MacOS", "visual-computer-use-mcp");
  const architectures = run("/usr/bin/lipo", ["-archs", executable], { capture: true }).split(/\s+/);
  const platforms = architectures.map(architecture => {
    if (architecture === "arm64") return "macos-arm64";
    if (architecture === "x86_64") return "macos-x64";
    throw new Error(`Unsupported app architecture: ${architecture}`);
  });
  if (JSON.stringify(platforms) !== JSON.stringify(expectedPlatforms)) {
    throw new Error(`macOS app architectures do not match ${expectedPlatforms.join(", ")}.`);
  }
  const appVersion = run(
    "/usr/libexec/PlistBuddy",
    ["-c", "Print :CFBundleShortVersionString", path.join(appPath, "Contents", "Info.plist")],
    { capture: true },
  );
  if (appVersion !== packageMetadata.version) {
    throw new Error(`App version ${appVersion} does not match ${packageMetadata.version}.`);
  }
  const service = readFileSync(
    path.join(projectRoot, "Sources", "VisualComputerUseMCP", "MCPService.swift"),
    "utf8",
  );
  if (readDeclaredVersion(service, "macOS MCP service") !== packageMetadata.version) {
    throw new Error("macOS MCP service version does not match the package version.");
  }
  return appPath;
}

function buildAndValidateWindows() {
  assertWindowsPackagingHost();
  if (!skipBuild) run(process.execPath, ["./scripts/build-windows-worker.mjs"]);
  const project = readFileSync(
    path.join(projectRoot, "windows", "VisualComputerUse.Windows", "VisualComputerUse.Windows.csproj"),
    "utf8",
  );
  const service = readFileSync(
    path.join(projectRoot, "windows", "VisualComputerUse.Windows", "McpService.cs"),
    "utf8",
  );
  for (const [source, label] of [[project, "Windows project"], [service, "Windows MCP service"]]) {
    if (readDeclaredVersion(source, label) !== packageMetadata.version) {
      throw new Error(`${label} version does not match the package version.`);
    }
  }
  for (const { directory } of Object.values(supportedWindowsRuntimes)) {
    const executable = path.join(projectRoot, "bin", directory, "open-computer-use.exe");
    if (windowsPlatformFromPortableExecutable(readFileSync(executable)) !== directory) {
      throw new Error(`${executable} contains the wrong architecture.`);
    }
  }
}

function stagePackage(nativeAppPath) {
  const stagingRoot = path.join(projectRoot, "dist", "package-staging", platform);
  rmSync(stagingRoot, { recursive: true, force: true });
  mkdirSync(path.join(stagingRoot, "bin"), { recursive: true });
  cpSync(path.join(projectRoot, "bin", "open-computer-use"), path.join(stagingRoot, "bin", "open-computer-use"));
  chmodSync(path.join(stagingRoot, "bin", "open-computer-use"), 0o755);
  cpSync(path.join(projectRoot, "skills"), path.join(stagingRoot, "skills"), { recursive: true });
  cpSync(path.join(projectRoot, "README.md"), path.join(stagingRoot, "README.md"));
  cpSync(path.join(projectRoot, "LICENSE"), path.join(stagingRoot, "LICENSE"));
  writeFileSync(path.join(stagingRoot, "package.json"), `${JSON.stringify(packageMetadata, null, 2)}\n`);
  writeFileSync(path.join(stagingRoot, "chatos.plugin.json"), `${JSON.stringify(manifest, null, 2)}\n`);

  if (platform === "macos") {
    const destination = path.join(stagingRoot, "dist", "Visual Computer Use.app");
    mkdirSync(path.dirname(destination), { recursive: true });
    cpSync(nativeAppPath, destination, { recursive: true, preserveTimestamps: true });
  } else {
    for (const { directory } of Object.values(supportedWindowsRuntimes)) {
      const destination = path.join(stagingRoot, "bin", directory);
      mkdirSync(destination, { recursive: true });
      cpSync(
        path.join(projectRoot, "bin", directory, "open-computer-use.exe"),
        path.join(destination, "open-computer-use.exe"),
      );
    }
  }
  return stagingRoot;
}

assertMetadata();
const nativeAppPath = platform === "macos"
  ? buildAndValidateMac()
  : (buildAndValidateWindows(), undefined);
run(process.execPath, [
  "./scripts/audit-skill-gates.mjs",
  "--manifest",
  platform === "macos" ? "chatos.plugin.json" : "windows/chatos.plugin.json",
]);
const stagingRoot = stagePackage(nativeAppPath);
const artifactDirectory = path.join(projectRoot, "dist", "chatos-artifacts");
mkdirSync(artifactDirectory, { recursive: true });
const artifactName = `${packageMetadata.name}-${packageMetadata.version}.tgz`;
rmSync(path.join(artifactDirectory, artifactName), { force: true });
run("npm", ["pack", stagingRoot, "--pack-destination", artifactDirectory]);
console.log(`ChatOS ${platform} artifact: ${path.join(artifactDirectory, artifactName)}`);
