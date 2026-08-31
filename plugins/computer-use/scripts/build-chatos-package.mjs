#!/usr/bin/env node

import { chmodSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

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

run("./scripts/build-macos-app.sh", []);

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

chmodSync(path.join(projectRoot, "bin", "open-computer-use"), 0o755);
const artifactDirectory = path.join(projectRoot, "dist", "chatos-artifacts");
mkdirSync(artifactDirectory, { recursive: true });
const artifactName = `${packageMetadata.name}-${packageMetadata.version}.tgz`;
rmSync(path.join(artifactDirectory, artifactName), { force: true });
run("npm", ["pack", "--pack-destination", artifactDirectory]);
console.log(`ChatOS artifact: ${path.join(artifactDirectory, artifactName)}`);
