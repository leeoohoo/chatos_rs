import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {withChromeSession, createVerificationTabs} from './chrome-verification-contract.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const extensionId = process.env.CHATOS_BROWSER_EXTENSION_ID;
if (!/^[a-p]{32}$/.test(extensionId ?? '')) {
  throw new Error('CHATOS_BROWSER_EXTENSION_ID must be a 32-character Chrome extension ID');
}

const binary = process.env.CHATOS_BROWSER_BINARY ?? path.join(root, 'target', 'release', 'chatos-browser-cdp');
const child = spawn(binary, ['mcp'], {
  cwd: root,
  env: {...process.env, CHATOS_BROWSER_EXTENSION_ID: extensionId},
  shell: false,
  stdio: ['pipe', 'pipe', 'pipe']
});
const lines = createInterface({input: child.stdout});
const pending = new Map();
let nextId = 1;
let stderr = '';

child.stderr.setEncoding('utf8');
child.stderr.on('data', chunk => {
  stderr = `${stderr}${chunk}`.slice(-64 * 1024);
});
lines.on('line', line => {
  const message = JSON.parse(line);
  const request = pending.get(String(message.id));
  if (!request) return;
  pending.delete(String(message.id));
  request.resolve(message);
});
child.on('exit', (code, signal) => {
  const error = new Error(`Browser MCP exited before verification completed (${signal ?? code})\n${stderr}`);
  for (const request of pending.values()) request.reject(error);
  pending.clear();
});

try {
  await request('initialize', {
    protocolVersion: '2025-06-18',
    capabilities: {},
    clientInfo: {name: 'verify-current-chrome', version: '0.1.2'}
  });

  await withChromeSession(callTool, 'Browser MCP 标签组验收', async session => {
    const tabs = await createVerificationTabs(callTool);
    process.stdout.write(`${JSON.stringify({phase: 'ready', session, tabs})}\n`);
    await new Promise(resolve => {
      const finish = () => {
        clearTimeout(timeout);
        process.stdin.off('data', finish);
        resolve();
      };
      const timeout = setTimeout(finish, 60_000);
      process.stdin.once('data', finish);
    });
  });
  process.stdout.write(`${JSON.stringify({phase: 'closed'})}\n`);
  await request('shutdown', {});
} finally {
  child.kill('SIGTERM');
}

function request(method, params) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      pending.delete(String(id));
      reject(new Error(`${method} timed out\n${stderr}`));
    }, 20_000);
    pending.set(String(id), {
      resolve: message => {
        clearTimeout(timeout);
        if (message.error) reject(new Error(message.error.message ?? JSON.stringify(message.error)));
        else resolve(message.result);
      },
      reject: error => {
        clearTimeout(timeout);
        reject(error);
      }
    });
    child.stdin.write(`${JSON.stringify({jsonrpc: '2.0', id, method, params})}\n`);
  });
}

async function callTool(name, arguments_) {
  const result = await request('tools/call', {name, arguments: arguments_});
  if (result?.isError) throw new Error(result.content?.[0]?.text ?? `${name} failed`);
  return result?.structuredContent;
}
