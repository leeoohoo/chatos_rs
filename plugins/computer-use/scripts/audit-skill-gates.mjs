#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const auditScript = path.resolve(
  packageRoot,
  "..",
  "..",
  "scripts",
  "audit-plugin-skill-gates.mjs",
);

function nativeMcpCommand() {
  if (process.platform === "win32") {
    return [
      process.execPath,
      path.join(packageRoot, "bin", "open-computer-use"),
      "mcp",
    ];
  }
  if (process.platform === "darwin") {
    return ["swift", "run", "visual-computer-use-mcp"];
  }
  throw new Error(
    `Computer Use skill-gate audit requires macOS or Windows; received ${process.platform}.`,
  );
}

const result = spawnSync(
  process.execPath,
  [
    auditScript,
    "--manifest",
    "chatos.plugin.json",
    "--cwd",
    ".",
    "--",
    ...nativeMcpCommand(),
  ],
  {
    cwd: packageRoot,
    stdio: "inherit",
    windowsHide: true,
  },
);

if (result.error) throw result.error;
process.exit(result.status ?? 1);
