#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform !== "win32") {
  throw new Error("The packaged Windows worker smoke test must run on Windows.");
}

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptDirectory, "..");
const result = spawnSync(
  process.execPath,
  [path.join(projectRoot, "bin", "open-computer-use"), "--doctor"],
  {
    cwd: projectRoot,
    encoding: "utf8",
    timeout: 15_000,
    maxBuffer: 256 * 1_024,
    windowsHide: true,
  },
);
if (result.error) throw result.error;
if (result.status !== 0) {
  throw new Error(`Windows worker doctor exited with status ${result.status}.`);
}
const diagnostics = JSON.parse(result.stdout.trim());
for (const field of [
  "screenCaptureAvailable",
  "inputDesktopAvailable",
  "processElevated",
  "allGranted",
]) {
  if (typeof diagnostics[field] !== "boolean") {
    throw new Error(`Windows worker doctor did not return boolean ${field}.`);
  }
}
if (
  diagnostics.allGranted !==
  (diagnostics.screenCaptureAvailable && diagnostics.inputDesktopAvailable)
) {
  throw new Error("Windows worker doctor returned inconsistent capabilities.");
}
if (diagnostics.applicationName !== "Visual Computer Use for Windows") {
  throw new Error("Windows worker doctor returned an unexpected application identity.");
}
console.log("Packaged Windows Computer Use worker started and returned valid diagnostics.");
