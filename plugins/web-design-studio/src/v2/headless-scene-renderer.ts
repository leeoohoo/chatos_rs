import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import { fileURLToPath } from 'node:url';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import WebSocket from 'ws';
import type { RenderedSceneMeasurements } from './layout-calibration.js';
import type { SceneRect } from './scene-schema.js';

export interface SceneImageCaptureInput {
  html: string;
  width: number;
  height: number;
  clip?: SceneRect;
}

export interface SceneImageCaptureResult {
  png: Buffer;
  width: number;
  height: number;
  measurements: RenderedSceneMeasurements;
}

export interface SceneImageRenderer {
  capture(input: SceneImageCaptureInput): Promise<SceneImageCaptureResult>;
}

interface CdpClient {
  send(method: string, params?: Record<string, unknown>): Promise<Record<string, any>>;
  close(): void;
}

interface CaptureAssetServer {
  url: string;
  close(): Promise<void>;
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function withTimeout<T>(promise: Promise<T>, milliseconds: number, message: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error(message)), milliseconds);
    promise.then(
      (value) => { clearTimeout(timeout); resolve(value); },
      (error) => { clearTimeout(timeout); reject(error); }
    );
  });
}

async function stopBrowser(child: ChildProcessWithoutNullStreams, timeoutMs = 5_000): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;
  const closed = new Promise<void>((resolve) => {
    child.once('close', () => resolve());
  });
  if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
  await withTimeout(closed, timeoutMs, 'Headless browser did not exit after it was stopped.');
}

export function resolveHeadlessBrowserExecutable(explicit = process.env.WEB_DESIGN_STUDIO_BROWSER): string {
  if (explicit?.trim()) return explicit.trim();
  const candidates = [
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
    '/usr/bin/google-chrome',
    '/usr/bin/google-chrome-stable',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
    'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe'
  ];
  const available = candidates.find((candidate) => existsSync(candidate));
  if (!available) throw new Error('No supported Chromium browser was found. Set WEB_DESIGN_STUDIO_BROWSER to a Chrome, Chromium, or Edge executable.');
  return available;
}

async function startBrowser(browser: string, profileDirectory: string, timeoutMs: number): Promise<{ child: ChildProcessWithoutNullStreams; port: number }> {
  const child = spawn(browser, [
    '--headless=new', '--disable-gpu', '--hide-scrollbars', '--no-first-run', '--no-default-browser-check',
    '--disable-background-networking', '--disable-component-update', '--disable-sync', '--metrics-recording-only',
    '--remote-debugging-port=0', `--user-data-dir=${profileDirectory}`, 'about:blank'
  ], { stdio: ['pipe', 'pipe', 'pipe'] });
  let diagnostic = '';
  const endpoint = new Promise<number>((resolve, reject) => {
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk) => {
      diagnostic = `${diagnostic}${chunk}`.slice(-12000);
      const match = diagnostic.match(/DevTools listening on ws:\/\/127\.0\.0\.1:(\d+)\//);
      if (match) resolve(Number(match[1]));
    });
    child.once('error', reject);
    child.once('exit', (code) => reject(new Error(`Headless browser exited before DevTools was ready with code ${code}. ${diagnostic.slice(-2000)}`)));
  });
  try {
    return { child, port: await withTimeout(endpoint, timeoutMs, 'Headless browser startup timed out.') };
  } catch (error) {
    await stopBrowser(child).catch(() => undefined);
    throw error;
  }
}

async function pageWebSocketUrl(port: number, timeoutMs: number): Promise<string> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`);
      const targets = await response.json() as Array<{ type?: string; webSocketDebuggerUrl?: string }>;
      const page = targets.find((target) => target.type === 'page' && target.webSocketDebuggerUrl);
      if (page?.webSocketDebuggerUrl) return page.webSocketDebuggerUrl;
    } catch {
      // DevTools may announce its port before the HTTP endpoint is ready.
    }
    await delay(50);
  }
  throw new Error('Headless browser page target did not become available.');
}

async function connectCdp(webSocketUrl: string, timeoutMs: number): Promise<CdpClient> {
  const socket = new WebSocket(webSocketUrl);
  await withTimeout(new Promise<void>((resolve, reject) => {
    socket.once('open', () => resolve());
    socket.once('error', reject);
  }), timeoutMs, 'DevTools WebSocket connection timed out.');
  let nextId = 1;
  const pending = new Map<number, { resolve(value: Record<string, any>): void; reject(error: Error): void }>();
  socket.on('message', (raw) => {
    const message = JSON.parse(raw.toString()) as { id?: number; result?: Record<string, any>; error?: { message: string } };
    if (!message.id) return;
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    if (message.error) request.reject(new Error(message.error.message));
    else request.resolve(message.result ?? {});
  });
  socket.on('close', () => {
    for (const request of pending.values()) request.reject(new Error('DevTools connection closed.'));
    pending.clear();
  });
  return {
    send(method, params = {}) {
      const id = nextId++;
      const response = new Promise<Record<string, any>>((resolve, reject) => pending.set(id, { resolve, reject }));
      socket.send(JSON.stringify({ id, method, params }));
      return withTimeout(response, timeoutMs, `DevTools command timed out: ${method}`);
    },
    close() { socket.terminate(); }
  };
}

function contentType(filename: string): string {
  const extension = path.extname(filename).toLowerCase();
  if (extension === '.html') return 'text/html; charset=utf-8';
  if (extension === '.js') return 'text/javascript; charset=utf-8';
  if (extension === '.css') return 'text/css; charset=utf-8';
  if (extension === '.json') return 'application/json; charset=utf-8';
  if (extension === '.svg') return 'image/svg+xml';
  if (extension === '.png') return 'image/png';
  if (extension === '.jpg' || extension === '.jpeg') return 'image/jpeg';
  if (extension === '.woff2') return 'font/woff2';
  if (extension === '.woff') return 'font/woff';
  return 'application/octet-stream';
}

async function listen(server: Server): Promise<number> {
  return await new Promise<number>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      if (!address || typeof address === 'string') reject(new Error('Headless capture asset server did not expose a TCP port.'));
      else resolve(address.port);
    });
  });
}

async function closeServer(server: Server): Promise<void> {
  await new Promise<void>((resolve) => server.close(() => resolve()));
}

async function startCaptureAssetServer(html: string, uiDirectory: string): Promise<CaptureAssetServer> {
  const root = path.resolve(uiDirectory);
  const index = path.join(root, 'index.html');
  if (!existsSync(index)) throw new Error(`Web Design Studio UI runtime is missing: ${index}`);
  const server = createServer(async (request, response) => {
    try {
      const requestUrl = new URL(request.url ?? '/', 'http://127.0.0.1');
      if (requestUrl.pathname === '/scene') {
        response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' });
        response.end(html);
        return;
      }
      const relative = requestUrl.pathname === '/' ? 'index.html' : decodeURIComponent(requestUrl.pathname.slice(1));
      const filename = path.resolve(root, relative);
      if (filename !== root && !filename.startsWith(`${root}${path.sep}`)) {
        response.writeHead(403).end();
        return;
      }
      const body = await readFile(filename);
      response.writeHead(200, { 'Content-Type': contentType(filename), 'Cache-Control': 'no-store' });
      response.end(body);
    } catch {
      response.writeHead(404).end();
    }
  });
  const port = await listen(server);
  return { url: `http://127.0.0.1:${port}/scene`, close: () => closeServer(server) };
}

async function waitForRenderedScene(client: CdpClient, timeoutMs: number): Promise<RenderedSceneMeasurements> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const ready = await client.send('Runtime.evaluate', {
        expression: `JSON.stringify({ready:document.querySelector('[data-scene-ready="true"]')!==null,error:[...document.querySelectorAll('[data-library-error]')].map((element)=>element.dataset.libraryRuntimeInstance+': '+element.dataset.libraryError),measurements:Object.fromEntries([...document.querySelectorAll('[data-scene-node-id]')].map((element)=>{const rect=element.getBoundingClientRect();const frame=element.querySelector('iframe[data-library-runtime-instance]');return [element.dataset.sceneNodeId,{rect:{x:rect.x,y:rect.y,width:rect.width,height:rect.height},scrollWidth:element.scrollWidth,scrollHeight:element.scrollHeight,renderedText:frame?.contentDocument?.body?.innerText??element.innerText??''}]}))})`,
        returnByValue: true
      });
      const state = JSON.parse(String(ready.result?.value ?? '{}')) as { ready?: boolean; error?: string[]; measurements?: RenderedSceneMeasurements };
      if (state.error?.length) throw new Error(`Library component runtime failed: ${state.error.join('; ')}`);
      if (state.ready && state.measurements) return state.measurements;
    } catch (error) {
      if (error instanceof Error && error.message.startsWith('Library component runtime failed:')) throw error;
      // Navigation can briefly replace the execution context while the runtime page loads.
    }
    await delay(50);
  }
  throw new Error('Rendered Scene and its library components did not become ready for capture.');
}

function finiteDimension(value: number, label: string, maximum: number): number {
  if (!Number.isFinite(value) || value <= 0 || value > maximum) throw new Error(`${label} is invalid.`);
  return value;
}

export class ChromiumSceneImageRenderer implements SceneImageRenderer {
  constructor(
    private readonly browser?: string,
    private readonly timeoutMs = 30_000,
    private readonly uiDirectory = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../ui')
  ) {}

  async capture(input: SceneImageCaptureInput): Promise<SceneImageCaptureResult> {
    const width = Math.ceil(finiteDimension(input.width, 'capture width', 10000));
    const height = Math.ceil(finiteDimension(input.height, 'capture height', 50000));
    const clip = input.clip
      ? {
          x: Math.max(0, input.clip.x), y: Math.max(0, input.clip.y),
          width: finiteDimension(input.clip.width, 'clip width', 10000),
          height: finiteDimension(input.clip.height, 'clip height', 50000)
        }
      : { x: 0, y: 0, width, height };
    if (clip.x + clip.width > width + 0.5 || clip.y + clip.height > height + 0.5) throw new Error('Capture clip exceeds the rendered page bounds.');
    const profile = await mkdtemp(path.join(os.tmpdir(), 'web-design-scene-chrome-'));
    let child: ChildProcessWithoutNullStreams | undefined;
    let client: CdpClient | undefined;
    let assetServer: CaptureAssetServer | undefined;
    let captureFailed = false;
    try {
      const running = await startBrowser(this.browser ?? resolveHeadlessBrowserExecutable(), profile, this.timeoutMs);
      child = running.child;
      client = await connectCdp(await pageWebSocketUrl(running.port, this.timeoutMs), this.timeoutMs);
      await client.send('Page.enable');
      await client.send('Runtime.enable');
      await client.send('Emulation.setDeviceMetricsOverride', {
        width,
        height: Math.min(height, 1200),
        deviceScaleFactor: 1,
        mobile: width <= 600
      });
      await client.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });
      if (input.html.includes('data-library-runtime-instance')) {
        assetServer = await startCaptureAssetServer(input.html, this.uiDirectory);
        await client.send('Page.navigate', { url: assetServer.url });
      } else {
        const tree = await client.send('Page.getFrameTree');
        await client.send('Page.setDocumentContent', { frameId: tree.frameTree.frame.id, html: input.html });
      }
      const measurements = await waitForRenderedScene(client, this.timeoutMs);
      await client.send('Runtime.evaluate', {
        expression: 'document.fonts?.ready ?? Promise.resolve()', awaitPromise: true, returnByValue: true
      });
      await delay(100);
      const screenshot = await client.send('Page.captureScreenshot', {
        format: 'png', fromSurface: true, captureBeyondViewport: true,
        clip: { x: clip.x, y: clip.y, width: clip.width, height: clip.height, scale: 1 }
      });
      const png = Buffer.from(String(screenshot.data), 'base64');
      if (png.byteLength === 0) throw new Error('Headless browser returned an empty Scene image.');
      return { png, width: Math.ceil(clip.width), height: Math.ceil(clip.height), measurements };
    } catch (error) {
      captureFailed = true;
      throw error;
    } finally {
      let cleanupError: unknown;
      try { client?.close(); } catch (error) { cleanupError ??= error; }
      try { if (child) await stopBrowser(child); } catch (error) { cleanupError ??= error; }
      try { await assetServer?.close(); } catch (error) { cleanupError ??= error; }
      try {
        await rm(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
      } catch (error) {
        cleanupError ??= error;
      }
      if (!captureFailed && cleanupError) throw cleanupError;
    }
  }
}
