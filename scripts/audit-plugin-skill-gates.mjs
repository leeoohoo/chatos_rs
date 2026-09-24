#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, readFile, readdir, realpath, rm, stat } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

const SKILL_NAME = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const FORBIDDEN_MODEL_FIELDS = new Set([
  'activationRef', 'activation_ref', 'requiredSkillRefs', 'required_skill_refs',
  'skillEvidence', 'skill_evidence', 'skillRef', 'skill_ref'
]);

function fail(message) {
  process.stderr.write(`[plugin-skill-gate-audit] ${message}\n`);
  process.exitCode = 1;
}

function parseArguments(argv) {
  let manifest;
  let cwd;
  let timeoutMs = 20_000;
  let index = 0;
  for (; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === '--') {
      index += 1;
      break;
    }
    if (value === '--manifest') manifest = argv[++index];
    else if (value === '--cwd') cwd = argv[++index];
    else if (value === '--timeout-ms') timeoutMs = Number(argv[++index]);
    else throw new Error(`unknown option ${value}`);
  }
  const command = argv.slice(index);
  if (!manifest || command.length === 0) {
    throw new Error('usage: audit-plugin-skill-gates.mjs --manifest PATH [--cwd PATH] -- COMMAND [ARGS...]');
  }
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1_000 || timeoutMs > 120_000) {
    throw new Error('--timeout-ms must be an integer between 1000 and 120000');
  }
  return {
    manifestPath: path.resolve(manifest),
    cwd: path.resolve(cwd ?? path.dirname(path.resolve(manifest))),
    timeoutMs,
    command
  };
}

async function collectSkillDocuments(root) {
  const info = await stat(root);
  if (!info.isDirectory()) throw new Error(`Skill path is not a directory: ${root}`);
  const direct = path.join(root, 'SKILL.md');
  try {
    if ((await stat(direct)).isFile()) return [direct];
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  const documents = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    documents.push(...await collectSkillDocuments(path.join(root, entry.name)));
  }
  return documents;
}

function frontmatterValue(frontmatter, key) {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = frontmatter.match(new RegExp(`^\\s*${escapedKey}:\\s*(.+?)\\s*$`, 'm'));
  const value = match?.[1]?.trim();
  if (!value) return undefined;
  if ((value.startsWith('"') && value.endsWith('"'))
    || (value.startsWith("'") && value.endsWith("'"))) return value.slice(1, -1).trim();
  return value;
}

async function loadSkills(manifest, manifestPath) {
  if (!Array.isArray(manifest.skills) || manifest.skills.length === 0) {
    throw new Error('Plugin Manifest must declare at least one Skill path');
  }
  const packageRoot = await realpath(path.dirname(manifestPath));
  const documents = [];
  for (const entry of manifest.skills) {
    const relativePath = typeof entry === 'string' ? entry : entry?.path;
    if (typeof relativePath !== 'string' || relativePath.trim() === '') {
      throw new Error('Plugin Manifest contains an invalid Skill path');
    }
    const resolved = await realpath(path.resolve(packageRoot, relativePath));
    const relative = path.relative(packageRoot, resolved);
    if (relative.startsWith('..') || path.isAbsolute(relative)) {
      throw new Error(`Skill path escapes the package: ${relativePath}`);
    }
    documents.push(...await collectSkillDocuments(resolved));
  }
  const skills = new Map();
  for (const documentPath of documents) {
    const raw = await readFile(documentPath, 'utf8');
    const match = raw.replaceAll('\r\n', '\n').match(/^---\n([\s\S]*?)\n---\n([\s\S]+)$/);
    if (!match) throw new Error(`Skill has invalid or empty frontmatter/body: ${documentPath}`);
    const name = frontmatterValue(match[1], 'name');
    const description = frontmatterValue(match[1], 'description');
    if (!name || !SKILL_NAME.test(name) || name.length > 64) {
      throw new Error(`Skill has an invalid name: ${documentPath}`);
    }
    if (!description || match[2].trim() === '') {
      throw new Error(`Skill must have a description and instructions: ${documentPath}`);
    }
    if (path.basename(path.dirname(documentPath)) !== name) {
      throw new Error(`Skill name must match its directory: ${name}`);
    }
    if (skills.has(name)) throw new Error(`duplicate Skill name ${name}`);
    const role = frontmatterValue(match[1], 'chatos.role') ?? 'leaf';
    if (!['router', 'leaf'].includes(role)) throw new Error(`Skill ${name} has invalid chatos.role ${role}`);
    skills.set(name, {
      sha256: createHash('sha256').update(raw).digest('hex'),
      role
    });
  }
  if (skills.size === 0) throw new Error('Plugin Manifest Skill paths contain no SKILL.md files');
  if (![...skills.values()].some((skill) => skill.role === 'router')) {
    throw new Error('Plugin must declare at least one router Skill');
  }
  return skills;
}

function send(child, message) {
  child.stdin.write(`${JSON.stringify(message)}\n`);
}

async function listMcpTools({ command, cwd, timeoutMs }, temporaryRoot) {
  const environment = {
    ...process.env,
    CHATOS_CONTEXT_SCOPE: 'project',
    CHATOS_CONTEXT_SCOPE_ID: 'plugin-skill-audit-scope',
    CHATOS_PLUGIN_ARTIFACT_DIR: path.join(temporaryRoot, 'artifacts'),
    CHATOS_PLUGIN_DATA_DIR: path.join(temporaryRoot, 'data'),
    CHATOS_PROJECT_ID: 'plugin-skill-audit-project',
    CHATOS_PROJECT_NAME: 'Plugin Skill Audit',
    CHATOS_WORKSPACE: path.join(temporaryRoot, 'workspace'),
    CHATOS_WORKSPACE_ID: 'plugin-skill-audit-workspace',
    DIAGRAM_STUDIO_DATA_DIR: path.join(temporaryRoot, 'diagram'),
    DOCUMENT_MCP_DATA_DIR: path.join(temporaryRoot, 'document'),
    SOLUTION_STUDIO_DATA_DIR: path.join(temporaryRoot, 'solution'),
    WEB_DESIGN_STUDIO_DATA_DIR: path.join(temporaryRoot, 'web-design')
  };
  await mkdir(environment.CHATOS_WORKSPACE, { recursive: true });
  await mkdir(environment.CHATOS_PLUGIN_ARTIFACT_DIR, { recursive: true });
  const child = spawn(command[0], command.slice(1), {
    cwd,
    env: environment,
    stdio: ['pipe', 'pipe', 'pipe']
  });
  let stdout = '';
  let stderr = '';
  const responses = new Map();
  let notify;
  const changed = () => new Promise((resolve) => { notify = resolve; });
  let nextChange = changed();
  child.stdout.setEncoding('utf8');
  child.stderr.setEncoding('utf8');
  child.stdout.on('data', (chunk) => {
    stdout += chunk;
    const lines = stdout.split(/\r?\n/);
    stdout = lines.pop() ?? '';
    for (const line of lines) {
      if (line.trim() === '') continue;
      try {
        const message = JSON.parse(line);
        if (message.id !== undefined) responses.set(String(message.id), message);
      } catch {
        stderr += `non-JSON stdout: ${line}\n`;
      }
    }
    notify?.();
    nextChange = changed();
  });
  child.stderr.on('data', (chunk) => { stderr = `${stderr}${chunk}`.slice(-16_000); });
  const exited = new Promise((resolve) => {
    child.once('exit', (code, signal) => resolve({ code, signal }));
    child.once('error', (error) => resolve({ error }));
  });
  const deadline = Date.now() + timeoutMs;
  const waitFor = async (id) => {
    while (!responses.has(String(id))) {
      if (Date.now() >= deadline) throw new Error(`MCP request ${id} timed out`);
      await Promise.race([
        nextChange,
        exited.then(({ code, signal, error }) => {
          if (error) throw new Error(`MCP failed to start: ${error.message}`);
          throw new Error(`MCP exited before response ${id}: ${code ?? signal}`);
        }),
        new Promise((resolve) => setTimeout(resolve, Math.min(250, deadline - Date.now())))
      ]);
    }
    const response = responses.get(String(id));
    if (response.error) throw new Error(`MCP request ${id} failed: ${JSON.stringify(response.error)}`);
    return response.result;
  };
  try {
    send(child, {
      jsonrpc: '2.0', id: 1, method: 'initialize',
      params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'chatos-plugin-skill-audit', version: '1.0.0' } }
    });
    await waitFor(1);
    send(child, { jsonrpc: '2.0', method: 'notifications/initialized', params: {} });
    send(child, { jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} });
    const result = await waitFor(2);
    if (!Array.isArray(result?.tools)) throw new Error('MCP tools/list returned no tools array');
    return result.tools;
  } catch (error) {
    const diagnostic = stderr.trim();
    throw new Error(`${error.message}${diagnostic ? `\nMCP stderr:\n${diagnostic}` : ''}`);
  } finally {
    child.stdin.end();
    child.kill('SIGTERM');
    await Promise.race([exited, new Promise((resolve) => setTimeout(resolve, 500))]);
    if (child.exitCode === null) child.kill('SIGKILL');
  }
}

function validateGate(tool, skills) {
  const gate = tool?._meta?.['chatos/skillGate'];
  if (!gate || typeof gate !== 'object' || Array.isArray(gate)) {
    throw new Error(`${tool.name}: missing _meta.chatos/skillGate`);
  }
  const unknown = Object.keys(gate).filter((key) => !['allOf', 'selectByArgument'].includes(key));
  if (unknown.length > 0) throw new Error(`${tool.name}: unknown gate fields ${unknown.join(', ')}`);
  const allOf = gate.allOf ?? [];
  if (!Array.isArray(allOf) || allOf.some((name) => typeof name !== 'string')) {
    throw new Error(`${tool.name}: allOf must be a string array`);
  }
  if (new Set(allOf).size !== allOf.length) throw new Error(`${tool.name}: allOf contains duplicates`);
  const referenced = [...allOf];
  const selector = gate.selectByArgument;
  if (selector !== undefined) {
    if (!selector || typeof selector !== 'object' || Array.isArray(selector)) {
      throw new Error(`${tool.name}: selectByArgument must be an object`);
    }
    if (Object.keys(selector).some((key) => !['pointer', 'map'].includes(key))
      || typeof selector.pointer !== 'string' || !selector.pointer.startsWith('/')
      || !selector.map || typeof selector.map !== 'object' || Array.isArray(selector.map)
      || Object.keys(selector.map).length === 0
      || Object.values(selector.map).some((name) => typeof name !== 'string')) {
      throw new Error(`${tool.name}: invalid selectByArgument declaration`);
    }
    referenced.push(...Object.values(selector.map));
  }
  if (referenced.length === 0) throw new Error(`${tool.name}: gate must require at least one Skill`);
  for (const name of referenced) {
    if (!SKILL_NAME.test(name) || !skills.has(name)) {
      throw new Error(`${tool.name}: gate references unknown Skill ${name}`);
    }
  }
  if (!referenced.some((name) => skills.get(name).role === 'router')) {
    throw new Error(`${tool.name}: gate must include a router Skill`);
  }
  return gate;
}

function inspectSchema(value, toolName, location = 'inputSchema') {
  if (!value || typeof value !== 'object') return;
  if (Array.isArray(value)) {
    value.forEach((item, index) => inspectSchema(item, toolName, `${location}[${index}]`));
    return;
  }
  if (value.properties && typeof value.properties === 'object') {
    for (const field of Object.keys(value.properties)) {
      if (FORBIDDEN_MODEL_FIELDS.has(field)) {
        throw new Error(`${toolName}: model schema exposes internal field ${field} at ${location}`);
      }
    }
  }
  for (const [key, child] of Object.entries(value)) inspectSchema(child, toolName, `${location}.${key}`);
}

function gateSignature(gate) {
  return JSON.stringify({
    allOf: [...(gate.allOf ?? [])].sort(),
    selectByArgument: gate.selectByArgument ?? null
  });
}

function auditTools(tools, skills) {
  if (tools.length === 0) throw new Error('MCP exposes no tools');
  const byName = new Map();
  const gates = new Map();
  for (const tool of tools) {
    if (!tool || typeof tool.name !== 'string' || tool.name.trim() === '') throw new Error('MCP exposes a tool without a name');
    if (byName.has(tool.name)) throw new Error(`duplicate MCP tool name ${tool.name}`);
    byName.set(tool.name, tool);
    gates.set(tool.name, validateGate(tool, skills));
    inspectSchema(tool.inputSchema, tool.name);
  }
  for (const tool of tools) {
    const canonical = tool?._meta?.['chatos/canonicalTool'];
    if (canonical === undefined) continue;
    if (typeof canonical !== 'string' || !byName.has(canonical) || canonical === tool.name) {
      throw new Error(`${tool.name}: invalid chatos/canonicalTool alias target`);
    }
    const aliasRequired = new Set(gates.get(tool.name).allOf ?? []);
    const canonicalRequired = gates.get(canonical).allOf ?? [];
    if (canonicalRequired.some((name) => !aliasRequired.has(name))) {
      throw new Error(`${tool.name}: alias gate is weaker than canonical tool ${canonical}`);
    }
    if (gateSignature({ selectByArgument: gates.get(tool.name).selectByArgument })
      !== gateSignature({ selectByArgument: gates.get(canonical).selectByArgument })) {
      throw new Error(`${tool.name}: alias selector differs from canonical tool ${canonical}`);
    }
  }
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const manifest = JSON.parse(await readFile(options.manifestPath, 'utf8'));
  const skills = await loadSkills(manifest, options.manifestPath);
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), 'chatos-plugin-skill-audit-'));
  try {
    const tools = await listMcpTools(options, temporaryRoot);
    auditTools(tools, skills);
    const digest = createHash('sha256')
      .update([...skills].map(([name, item]) => `${name}:${item.sha256}`).sort().join('\n'))
      .digest('hex');
    process.stdout.write(`[OK] ${manifest.name}: ${tools.length} tools, ${skills.size} Skills, catalog ${digest}\n`);
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

main().catch((error) => fail(error instanceof Error ? error.message : String(error)));
