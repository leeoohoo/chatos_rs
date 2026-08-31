import {access, mkdtemp, readFile, rm} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const extensionId = process.env.CHATOS_BROWSER_EXTENSION_ID;
if (!/^[a-p]{32}$/.test(extensionId ?? '')) {
  throw new Error('CHATOS_BROWSER_EXTENSION_ID must be the ID of the unpacked or production extension');
}
const extensionRoot = path.join(root, 'extension', 'dist');
const mcpBinary = path.join(root, 'target', 'debug', executable('chatos-browser-cdp'));
async function main() {
  const chrome = await findChrome();
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), 'chatos-bridge-e2e-'));
  let chromeProcess;
  let mcp;
  let cdp;

  try {
  await run('cargo', ['build', '-p', 'browser-cdp-cli'], root);
  await run(process.execPath, ['scripts/build.mjs'], path.join(root, 'extension'));
  mcp = new JsonLineProcess(
    spawn(mcpBinary, ['mcp'], {
      cwd: root,
      env: {...process.env, CHATOS_BROWSER_EXTENSION_ID: extensionId},
      shell: false,
      stdio: ['pipe', 'pipe', 'pipe']
    })
  );
  await mcp.request('initialize', {
    protocolVersion: '2025-06-18',
    capabilities: {},
    clientInfo: {name: 'browser-bridge-e2e', version: '0.1.0'}
  });

  const profile = path.join(temporaryRoot, 'chrome-profile');
  const chromeArguments = [
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-gpu',
      `--user-data-dir=${profile}`,
      `--disable-extensions-except=${extensionRoot}`,
      `--load-extension=${extensionRoot}`,
      '--remote-debugging-port=0',
      'about:blank'
  ];
  if (process.env.CHATOS_EXTENSION_E2E_HEADLESS === '1') {
    chromeArguments.unshift('--headless=new');
  }
  chromeProcess = spawn(
    chrome,
    chromeArguments,
    {shell: false, stdio: ['ignore', 'ignore', 'pipe']}
  );
  const activePort = path.join(profile, 'DevToolsActivePort');
  await waitForFile(activePort, 10_000);
  const [port] = (await readFile(activePort, 'utf8')).split(/\r?\n/);
  const extensionTarget = await waitForExtensionTarget(port, 10_000);
  cdp = await CdpClient.connect(extensionTarget.webSocketDebuggerUrl);
  const connected = await cdp.evaluate(`(async () => {
    try {
      return JSON.stringify(await chrome.runtime.sendMessage({action: 'connect'}));
    } catch {
      await import(chrome.runtime.getURL('src/background.js'));
      return JSON.stringify(await chrome.runtime.sendMessage({action: 'connect'}));
    }
  })()`);
  const connectionResult = JSON.parse(connected);
  if (!connectionResult.ok || !connectionResult.result.connected) {
    throw new Error(`Extension pairing failed: ${connected}`);
  }

  const opened = await mcp.callTool('browser_session_open', {mode: 'chrome_extension'});
  const browserSessionId = opened.browser_session_id;
  const evaluated = await mcp.callTool('browser_cdp_send', {
    browser_session_id: browserSessionId,
    method: 'Runtime.evaluate',
    params: {expression: "document.title = 'Bridge E2E'; document.title", returnByValue: true}
  });
  if (evaluated.result?.value !== 'Bridge E2E') {
    throw new Error(`Unexpected CDP result: ${JSON.stringify(evaluated)}`);
  }
  await mcp.callTool('browser_session_close', {browser_session_id: browserSessionId});
  await mcp.request('shutdown', {});
  process.stdout.write(`Self-managed Browser Bridge E2E passed\nExtension: ${extensionTarget.url}\n`);
  } finally {
    cdp?.close();
    await stop(mcp?.child);
    await stop(chromeProcess);
    await rm(temporaryRoot, {recursive: true, force: true});
    await run(mcpBinary, ['uninstall-native-host', extensionId], root).catch(() => {});
  }
}

class JsonLineProcess {
  constructor(child) {
    this.child = child;
    this.nextId = 1;
    this.pending = new Map();
    this.lines = createInterface({input: child.stdout});
    this.lines.on('line', line => {
      const message = JSON.parse(line);
      const callback = this.pending.get(String(message.id));
      if (callback) {
        this.pending.delete(String(message.id));
        callback.resolve(message);
      }
    });
  }

  request(method, params) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(String(id));
        reject(new Error(`MCP request ${method} timed out`));
      }, 20_000);
      this.pending.set(String(id), {
        resolve: message => {
          clearTimeout(timeout);
          resolve(message);
        }
      });
      this.child.stdin.write(`${JSON.stringify({jsonrpc: '2.0', id, method, params})}\n`);
    });
  }

  async callTool(name, arguments_) {
    const response = await this.request('tools/call', {name, arguments: arguments_});
    if (response.result?.isError) {
      throw new Error(response.result.content?.[0]?.text ?? `${name} failed`);
    }
    return response.result.structuredContent;
  }
}

class CdpClient {
  static async connect(endpoint) {
    const socket = new WebSocket(endpoint);
    await new Promise((resolve, reject) => {
      socket.addEventListener('open', resolve, {once: true});
      socket.addEventListener('error', reject, {once: true});
    });
    return new CdpClient(socket);
  }

  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    socket.addEventListener('message', event => {
      const message = JSON.parse(event.data);
      const callback = this.pending.get(message.id);
      if (!callback) return;
      this.pending.delete(message.id);
      if (message.error) callback.reject(new Error(message.error.message));
      else callback.resolve(message.result);
    });
  }

  command(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, {resolve, reject});
      this.socket.send(JSON.stringify({id, method, params}));
    });
  }

  async evaluate(expression) {
    const result = await this.command('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true
    });
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  }

  close() {
    this.socket.close();
  }
}

async function waitForExtensionTarget(port, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const response = await fetch(`http://127.0.0.1:${port}/json/list`);
    const targets = await response.json();
    const target = targets.find(candidate =>
      String(candidate.url ?? '').endsWith('/src/background.js')
    );
    if (target) return target;
    await delay(50);
  }
  throw new Error('Chrome extension background target did not appear');
}

async function waitForFile(file, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      await access(file);
      return;
    } catch {
      await delay(50);
    }
  }
  throw new Error(`Timed out waiting for ${file}`);
}

async function findChrome() {
  const candidates = [
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge'
  ];
  for (const candidate of candidates) {
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Continue without downloading a browser.
    }
  }
  throw new Error('No installed Chrome, Chromium, or Edge binary was found');
}

async function run(command, args, cwd) {
  const child = spawn(command, args, {cwd, shell: false, stdio: 'inherit'});
  const code = await new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', resolve);
  });
  if (code !== 0) throw new Error(`${command} exited with ${code}`);
}

async function stop(child) {
  if (!child || child.exitCode !== null) return;
  child.kill('SIGTERM');
  await Promise.race([
    new Promise(resolve => child.once('exit', resolve)),
    delay(2000)
  ]);
  if (child.exitCode === null) child.kill('SIGKILL');
}

function executable(name) {
  return process.platform === 'win32' ? `${name}.exe` : name;
}

function delay(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

await main();
