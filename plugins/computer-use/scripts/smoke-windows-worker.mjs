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
const requireScreenObservation = process.argv.includes("--screen");
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
    const requests = [
      JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "smoke", version: "1" } },
      }),
      JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} }),
      JSON.stringify({ jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "check_permissions", arguments: {} } }),
    ];
    if (requireScreenObservation) {
      requests.push(JSON.stringify({ jsonrpc: "2.0", id: 4, method: "tools/call", params: { name: "observe_screen", arguments: {} } }));
    }
    child.stdin.end(`${requests.join("\n")}\n`);
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
const expectedSkillGates = {
  check_permissions: ["visual-computer-use", "visual-safety-verification"],
  request_permissions: ["visual-computer-use", "visual-safety-verification"],
  observe_screen: ["visual-computer-use", "visual-observation-targeting"],
  move_mouse: ["visual-computer-use", "visual-observation-targeting"],
  click: ["visual-computer-use", "visual-pointer-keyboard"],
  scroll: ["visual-computer-use", "visual-pointer-keyboard"],
  type_text: ["visual-computer-use", "visual-pointer-keyboard"],
  key_press: ["visual-computer-use", "visual-pointer-keyboard"],
  active_application: ["visual-computer-use", "visual-application-focus"],
  activate_application: ["visual-computer-use", "visual-application-focus"],
  list_shortcuts: ["visual-computer-use", "visual-application-focus"],
};
for (const tool of tools) {
  const gate = tool?._meta?.["chatos/skillGate"]?.allOf;
  if (JSON.stringify(gate) !== JSON.stringify(expectedSkillGates[tool.name])) {
    throw new Error(`Windows worker tool ${tool.name} has an invalid Skill gate.`);
  }
}
const permissionResult = responses.find((response) => response.id === 3)?.result;
const permissionSnapshot = permissionResult?.structuredContent;
for (const field of ["screenCaptureAvailable", "inputDesktopAvailable", "processElevated", "allGranted"]) {
  if (typeof permissionSnapshot?.[field] !== "boolean") {
    throw new Error(`Windows worker MCP check_permissions did not return boolean ${field}.`);
  }
}
if (requireScreenObservation) {
  const observation = responses.find((response) => response.id === 4)?.result;
  const observationSnapshot = observation?.structuredContent;
  if (observation?.isError || !observationSnapshot || typeof observationSnapshot !== "object") {
    throw new Error("Windows worker MCP observe_screen did not return structured content.");
  }
  for (const field of [
    "globalDesktopBounds",
    "captureRegionGlobal",
    "virtualCursorGlobal",
    "cursorScreenshotPixel",
  ]) {
    if (!observationSnapshot[field] || typeof observationSnapshot[field] !== "object") {
      throw new Error(`Windows worker observe_screen did not return ${field}.`);
    }
  }
  for (const field of ["screenshotPixelWidth", "screenshotPixelHeight", "encodedByteCount"]) {
    if (!Number.isInteger(observationSnapshot[field]) || observationSnapshot[field] <= 0) {
      throw new Error(`Windows worker observe_screen returned an invalid ${field}.`);
    }
  }
  if (observationSnapshot.cursorMarkerIncluded !== true) {
    throw new Error("Windows worker observe_screen did not include the virtual cursor marker.");
  }
  const observationImage = observation.content?.find((item) => item.type === "image");
  if (!observationImage?.data || !observationImage.mimeType?.startsWith("image/")) {
    throw new Error("Windows worker observe_screen did not return an encoded screenshot image.");
  }
}
console.log(
  `Packaged Windows Computer Use worker passed doctor, MCP initialize/tools/list, and check_permissions${
    requireScreenObservation ? ", and observe_screen" : ""
  }.`,
);
