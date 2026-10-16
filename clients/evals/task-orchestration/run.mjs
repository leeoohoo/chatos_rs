#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { mkdtemp, rm, access } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const args = process.argv.slice(2);
const knownFlags = new Set(['--live', '--smoke', '--list', '--all', '--allow-loopback-http', '--help']);
const valuedFlags = new Set(['--fixture', '--dataset', '--output', '--case']);
let suppliedFixture = false;
for (let i = 0; i < args.length; i++) {
  if (valuedFlags.has(args[i])) {
    if (!args[i + 1] || args[i + 1].startsWith('--')) throw new Error(`Missing value for ${args[i]}`);
    if (args[i] === '--fixture') suppliedFixture = true;
    if (args[i] !== '--case') args[i + 1] = resolve(args[i + 1]);
    i++;
  } else if (!knownFlags.has(args[i])) throw new Error(`Unknown argument: ${args[i]}`);
}
// Inherited app/user secrets are never read. Only explicit CHATOS_EVAL_* reach the model.
function child(command, argv, cwd, env = process.env) {
  return new Promise((resolveExit, reject) => {
    const processChild = spawn(command, argv, { cwd, env, stdio: 'inherit' });
    processChild.once('error', reject);
    processChild.once('exit', (code, signal) => resolveExit(signal ? 2 : code ?? 2));
  });
}
let scratch;
try {
  if (!suppliedFixture && !args.includes('--list') && !args.includes('--help')) {
    if (process.platform !== 'darwin') throw new Error('Supply --fixture exported by the client on this platform.');
    scratch = await mkdtemp(join(tmpdir(), 'chatos-orchestration-eval-'));
    const fixture = join(scratch, 'frozen-capabilities.json');
    const mac = join(root, 'clients/macos');
    if (await child('swift', ['build', '--target', 'ChatOSConnectorTests'], mac)) throw new Error('Client test build failed.');
    const bundle = join(mac, '.build/out/Products/Debug/ChatOSConnectorTests.xctest');
    await access(bundle);
    const env = { ...process.env, CHATOS_ORCHESTRATION_EXPORT_PATH: fixture };
    // xctest does not need evaluation credentials; do not hand them to the exporter.
    for (const key of Object.keys(env)) if (key.startsWith('CHATOS_EVAL_')) delete env[key];
    if (await child('xcrun', ['xctest', '-XCTest',
      'ChatOSConnectorTests.NativeTaskOrchestrationEvalExportTests/testExportFrozenEvaluationCatalog', bundle], mac, env)) {
      throw new Error('Frozen catalog export failed.');
    }
    await access(fixture);
    args.push('--fixture', fixture);
  }
  process.exitCode = await child('cargo', ['run', '--quiet', '-p', 'chatos_local_agent_host',
    '--example', 'orchestration_eval', '--', ...args], root);
} catch (error) {
  // Do not print child environment or command-line configs.
  console.error(error.message);
  process.exitCode = 2;
} finally {
  if (scratch) await rm(scratch, { recursive: true, force: true });
}
