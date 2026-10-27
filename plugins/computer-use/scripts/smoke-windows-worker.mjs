#!/usr/bin/env node

import { spawn, spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform !== "win32") {
  throw new Error("The packaged Windows worker smoke test must run on Windows.");
}

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptDirectory, "..");
const packagedLauncher = path.join(projectRoot, "bin", "open-computer-use");
const result = spawnSync(
  process.execPath,
  [packagedLauncher, "--doctor"],
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

function readMcpHandshake() {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [packagedLauncher, "mcp"], {
      cwd: projectRoot,
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    const responses = [];
    let output = "";
    let settled = false;
    const timeout = setTimeout(() => {
      child.kill();
      reject(new Error("Windows worker MCP handshake timed out."));
    }, 15_000);
    const finish = (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      if (error) reject(error);
      else resolve(responses);
    };
    child.on("error", (error) => finish(error));
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      output += chunk;
      let newline;
      while ((newline = output.indexOf("\n")) >= 0) {
        const line = output.slice(0, newline).trim();
        output = output.slice(newline + 1);
        if (!line) continue;
        try {
          responses.push(JSON.parse(line));
        } catch (error) {
          finish(new Error(`Windows worker returned invalid MCP JSON: ${error.message}`));
          return;
        }
      }
    });
    child.on("close", (code) => {
      if (code !== 0) {
        finish(new Error(`Windows worker MCP process exited with status ${code}.`));
        return;
      }
      finish(null);
    });
    child.stdin.end([
      JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "smoke", version: "1" } },
      }),
      JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} }),
      "",
    ].join("\n"));
  });
}

const responses = await readMcpHandshake();
const initialize = responses.find((response) => response.id === 1);
if (!initialize?.result?.serverInfo || initialize.result.serverInfo.name !== "visual-computer-use") {
  throw new Error("Windows worker MCP initialize returned an unexpected server identity.");
}
const tools = responses.find((response) => response.id === 2)?.result?.tools;
const expectedTools = [
  "check_permissions",
  "request_permissions",
  "observe_screen",
  "move_mouse",
  "click",
  "scroll",
  "type_text",
  "key_press",
  "active_application",
  "activate_application",
  "list_shortcuts",
];
if (!Array.isArray(tools) || JSON.stringify(tools.map((tool) => tool.name)) !== JSON.stringify(expectedTools)) {
  throw new Error("Windows worker MCP tools/list did not expose the expected 11-tool contract.");
}
console.log("Packaged Windows Computer Use worker passed doctor and MCP initialize/tools/list.");
