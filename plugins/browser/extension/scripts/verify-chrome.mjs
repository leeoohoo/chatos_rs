import {access, mkdtemp, readFile, rm} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const extensionRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'dist');
const chrome = await findChrome();
const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), 'chatos-extension-test-'));
const profile = path.join(temporaryRoot, 'profile');
let stderr = '';
const child = spawn(
  chrome,
  [
    '--headless=new',
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-gpu',
    `--user-data-dir=${profile}`,
    `--disable-extensions-except=${extensionRoot}`,
    `--load-extension=${extensionRoot}`,
    '--remote-debugging-port=0',
    'about:blank'
  ],
  {shell: false, stdio: ['ignore', 'ignore', 'pipe'], windowsHide: true}
);
child.stderr.setEncoding('utf8');
child.stderr.on('data', chunk => {
  stderr = `${stderr}${chunk}`.slice(-64 * 1024);
});

try {
  const activePort = path.join(profile, 'DevToolsActivePort');
  await waitForFile(activePort, 10_000);
  const [port] = (await readFile(activePort, 'utf8')).split(/\r?\n/);
  const response = await fetch(`http://127.0.0.1:${port}/json/list`);
  if (!response.ok) throw new Error(`DevTools target query failed with HTTP ${response.status}`);
  const targets = await response.json();
  const extensionTargets = targets.filter(target =>
    String(target.url ?? '').startsWith('chrome-extension://')
  );
  const extensionTarget = extensionTargets.find(target =>
    String(target.url ?? '').endsWith('/src/background.js')
  );
  if (!extensionTarget) {
    throw new Error(
      `Chrome did not load the Chatos extension service worker. Extension targets: ${JSON.stringify(extensionTargets)}\n${stderr}`
    );
  }
  if (/Manifest is not valid|Failed to load extension|Invalid value for 'content_security_policy'/i.test(stderr)) {
    throw new Error(`Chrome rejected the extension manifest\n${stderr}`);
  }
  process.stdout.write(
    `Chrome loaded extension target ${extensionTarget.type} ${extensionTarget.url}\n`
  );
} finally {
  child.kill('SIGTERM');
  await Promise.race([
    new Promise(resolve => child.once('exit', resolve)),
    new Promise(resolve => setTimeout(resolve, 2000))
  ]);
  if (child.exitCode === null) child.kill('SIGKILL');
  await rm(temporaryRoot, {recursive: true, force: true});
}

async function findChrome() {
  const candidates = {
    darwin: [
      '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      '/Applications/Chromium.app/Contents/MacOS/Chromium',
      '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge'
    ],
    linux: ['/usr/bin/google-chrome', '/usr/bin/chromium', '/usr/bin/chromium-browser', '/usr/bin/microsoft-edge'],
    win32: [
      path.join(process.env.PROGRAMFILES ?? '', 'Google/Chrome/Application/chrome.exe'),
      path.join(process.env['PROGRAMFILES(X86)'] ?? '', 'Google/Chrome/Application/chrome.exe'),
      path.join(process.env.LOCALAPPDATA ?? '', 'Google/Chrome/Application/chrome.exe'),
      path.join(process.env.PROGRAMFILES ?? '', 'Microsoft/Edge/Application/msedge.exe')
    ]
  }[process.platform] ?? [];
  for (const candidate of candidates) {
    if (!candidate) continue;
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Continue through installed-browser candidates without downloading anything.
    }
  }
  throw new Error('No installed Chrome, Chromium, or Edge binary was found');
}

async function waitForFile(file, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      await access(file);
      return;
    } catch {
      await new Promise(resolve => setTimeout(resolve, 50));
    }
  }
  throw new Error(`Chrome did not create ${file} before timeout\n${stderr}`);
}
