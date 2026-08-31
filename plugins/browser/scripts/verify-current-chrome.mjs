import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

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

  const deadline = Date.now() + 30_000;
  let session;
  let lastError;
  while (Date.now() < deadline && !session) {
    try {
      session = await callTool('browser_session_open', {
        mode: 'chrome_extension',
        session_name: 'Browser MCP 标签组验收'
      });
    } catch (error) {
      lastError = error;
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
  }
  if (!session) throw lastError ?? new Error('Browser extension did not connect');

  const first = await callTool('browser_tab_new', {
    browser_session_id: session.browser_session_id,
    url: 'https://example.com/?browser-mcp-verification=one'
  });
  const second = await callTool('browser_tab_new', {
    browser_session_id: session.browser_session_id,
    url: 'https://example.com/?browser-mcp-verification=two'
  });
  process.stdout.write(`${JSON.stringify({phase: 'ready', session, tabs: [first, second]})}\n`);

  await Promise.race([
    new Promise(resolve => process.stdin.once('data', resolve)),
    new Promise(resolve => setTimeout(resolve, 60_000))
  ]);
  await callTool('browser_session_close', {browser_session_id: session.browser_session_id});
  process.stdout.write(`${JSON.stringify({phase: 'closed', browser_session_id: session.browser_session_id})}\n`);
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
      reject
    });
    child.stdin.write(`${JSON.stringify({jsonrpc: '2.0', id, method, params})}\n`);
  });
}

async function callTool(name, arguments_) {
  const result = await request('tools/call', {name, arguments: arguments_});
  if (result?.isError) throw new Error(result.content?.[0]?.text ?? `${name} failed`);
  return result?.structuredContent;
}
