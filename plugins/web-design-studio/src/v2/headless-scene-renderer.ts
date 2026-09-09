import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdtemp, rm } from 'node:fs/promises';
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
    child.kill('SIGKILL');
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

function finiteDimension(value: number, label: string, maximum: number): number {
  if (!Number.isFinite(value) || value <= 0 || value > maximum) throw new Error(`${label} is invalid.`);
  return value;
}

export class ChromiumSceneImageRenderer implements SceneImageRenderer {
  constructor(
    private readonly browser?: string,
    private readonly timeoutMs = 30_000
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
      const tree = await client.send('Page.getFrameTree');
      await client.send('Page.setDocumentContent', { frameId: tree.frameTree.frame.id, html: input.html });
      await client.send('Runtime.evaluate', {
        expression: 'document.fonts?.ready ?? Promise.resolve()', awaitPromise: true, returnByValue: true
      });
      const ready = await client.send('Runtime.evaluate', {
        expression: `JSON.stringify({ready:document.querySelector('[data-scene-ready="true"]')!==null,measurements:Object.fromEntries([...document.querySelectorAll('[data-scene-node-id]')].map((element)=>{const rect=element.getBoundingClientRect();return [element.dataset.sceneNodeId,{rect:{x:rect.x,y:rect.y,width:rect.width,height:rect.height},scrollWidth:element.scrollWidth,scrollHeight:element.scrollHeight}]}))})`,
        returnByValue: true
      });
      const state = JSON.parse(String(ready.result?.value ?? '{}')) as { ready?: boolean; measurements?: RenderedSceneMeasurements };
      if (!state.ready || !state.measurements) throw new Error('Rendered Scene did not become ready for capture.');
      const screenshot = await client.send('Page.captureScreenshot', {
        format: 'png', fromSurface: true, captureBeyondViewport: true,
        clip: { x: clip.x, y: clip.y, width: clip.width, height: clip.height, scale: 1 }
      });
      const png = Buffer.from(String(screenshot.data), 'base64');
      if (png.byteLength === 0) throw new Error('Headless browser returned an empty Scene image.');
      return { png, width: Math.ceil(clip.width), height: Math.ceil(clip.height), measurements: state.measurements };
    } finally {
      client?.close();
      child?.kill('SIGKILL');
      await rm(profile, { recursive: true, force: true });
    }
  }
}
