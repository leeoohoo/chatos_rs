import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { PNG } from 'pngjs';
import WebSocket from 'ws';
import { createBaselineCapturePlan } from '../dist/v2-baseline-capture.test.mjs';

function argument(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

function resolveBrowser(explicit) {
  if (explicit) return explicit;
  if (process.env.WEB_DESIGN_BASELINE_BROWSER) return process.env.WEB_DESIGN_BASELINE_BROWSER;
  const absoluteCandidates = [
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge'
  ];
  return absoluteCandidates.find((candidate) => existsSync(candidate)) ?? 'google-chrome';
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function withTimeout(promise, milliseconds, message) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error(message)), milliseconds);
    promise.then(
      (value) => { clearTimeout(timeout); resolve(value); },
      (error) => { clearTimeout(timeout); reject(error); }
    );
  });
}

async function startBrowser(browser, profileDirectory, timeoutMs) {
  const child = spawn(browser, [
    '--headless=new',
    '--disable-gpu',
    '--hide-scrollbars',
    '--no-first-run',
    '--no-default-browser-check',
    '--remote-debugging-port=0',
    `--user-data-dir=${profileDirectory}`,
    'about:blank'
  ], { stdio: ['ignore', 'ignore', 'pipe'] });
  let diagnostic = '';
  const endpoint = new Promise((resolve, reject) => {
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk) => {
      diagnostic = `${diagnostic}${chunk}`.slice(-8000);
      const match = diagnostic.match(/DevTools listening on ws:\/\/127\.0\.0\.1:(\d+)\//);
      if (match) resolve({ port: Number(match[1]), diagnostic: () => diagnostic });
    });
    child.once('error', reject);
    child.once('exit', (code) => reject(new Error(`Browser exited before DevTools was ready with code ${code}.`)));
  });
  try {
    return { child, ...await withTimeout(endpoint, timeoutMs, 'Browser DevTools startup timed out.') };
  } catch (error) {
    child.kill('SIGKILL');
    throw error;
  }
}

async function findPageTarget(port, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`);
      const targets = await response.json();
      const page = targets.find((target) => target.type === 'page');
      if (page?.webSocketDebuggerUrl) return page.webSocketDebuggerUrl;
    } catch {
      // DevTools can start listening shortly before its HTTP endpoint is ready.
    }
    await delay(100);
  }
  throw new Error('Browser page target did not become available.');
}

async function createCdpClient(webSocketUrl, timeoutMs) {
  const socket = new WebSocket(webSocketUrl);
  await withTimeout(new Promise((resolve, reject) => {
    socket.once('open', resolve);
    socket.once('error', reject);
  }), timeoutMs, 'DevTools WebSocket connection timed out.');
  let nextId = 1;
  const pending = new Map();
  socket.on('message', (raw) => {
    const message = JSON.parse(raw.toString());
    if (!message.id) return;
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    if (message.error) request.reject(new Error(message.error.message));
    else request.resolve(message.result);
  });
  socket.on('close', () => {
    for (const request of pending.values()) request.reject(new Error('DevTools connection closed.'));
    pending.clear();
  });
  return {
    send(method, params = {}) {
      const id = nextId++;
      const response = new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
      socket.send(JSON.stringify({ id, method, params }));
      return withTimeout(response, timeoutMs, `DevTools command timed out: ${method}`);
    },
    close() { socket.terminate(); }
  };
}

async function waitForStudioMount(client, expectedUrl, timeoutMs) {
  const normalizedExpectedUrl = expectedUrl ? new URL(expectedUrl) : undefined;
  const deadline = Date.now() + timeoutMs;
  let lastState;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const result = await client.send('Runtime.evaluate', {
        expression: `JSON.stringify({
          readyState: document.readyState,
          url: location.href,
          mounted: Boolean(document.querySelector('#root')?.childElementCount),
          rendered: Boolean(document.querySelector('#root')?.childElementCount) || Boolean(document.body?.children.length),
          phase2Ready: document.documentElement.dataset.phase2Ready ?? null,
          bodyTextLength: document.body?.innerText?.trim().length ?? 0,
          viewport: { width: innerWidth, height: innerHeight },
          document: {
            scrollWidth: document.documentElement.scrollWidth,
            scrollHeight: document.documentElement.scrollHeight,
            clientWidth: document.documentElement.clientWidth,
            clientHeight: document.documentElement.clientHeight
          },
          designSurface: (() => {
            const element = document.querySelector('.page') ?? document.querySelector('#root')?.firstElementChild;
            if (!element) return null;
            const rect = element.getBoundingClientRect();
            return { left: rect.left, top: rect.top, width: rect.width, height: rect.height };
          })(),
          sceneNodes: [...document.querySelectorAll('[data-scene-node-id]')].map((element) => {
            const rect = element.getBoundingClientRect();
            return {
              nodeId: element.dataset.sceneNodeId,
              expected: {
                x: Number(element.dataset.expectedX),
                y: Number(element.dataset.expectedY),
                width: Number(element.dataset.expectedWidth),
                height: Number(element.dataset.expectedHeight)
              },
              rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
              scrollWidth: element.scrollWidth,
              scrollHeight: element.scrollHeight
            };
          })
        })`,
        returnByValue: true
      });
      if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text ?? 'Runtime evaluation failed.');
      if (typeof result.result?.value !== 'string') throw new Error(`Runtime evaluation returned ${result.result?.type ?? 'no value'}.`);
      const state = JSON.parse(result.result.value);
      lastState = state;
      const actualUrl = new URL(state.url);
      const samePage = !normalizedExpectedUrl
        || (actualUrl.origin === normalizedExpectedUrl.origin && actualUrl.pathname === normalizedExpectedUrl.pathname);
      const expectedQueryPreserved = !normalizedExpectedUrl || [...normalizedExpectedUrl.searchParams.entries()]
        .every(([key, value]) => actualUrl.searchParams.getAll(key).includes(value));
      const phase2Settled = state.phase2Ready === null || state.phase2Ready === 'true';
      if (samePage && expectedQueryPreserved && state.rendered && state.bodyTextLength > 0 && phase2Settled) return state;
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
      // Navigation replaces the JavaScript execution context; retry against the new one.
    }
    await delay(250);
  }
  throw new Error(`Studio root did not render before the page-ready timeout. Last state: ${JSON.stringify(lastState ?? null)}. Last error: ${lastError ?? 'none'}`);
}

function analyzeScreenshot(contents) {
  const png = PNG.sync.read(contents);
  const colors = new Set();
  let minimumLuminance = 255;
  let maximumLuminance = 0;
  const pixelCount = png.width * png.height;
  const sampleStep = Math.max(1, Math.floor(pixelCount / 5000));
  for (let pixel = 0; pixel < pixelCount; pixel += sampleStep) {
    const offset = pixel * 4;
    const red = png.data[offset];
    const green = png.data[offset + 1];
    const blue = png.data[offset + 2];
    const alpha = png.data[offset + 3];
    colors.add(`${red},${green},${blue},${alpha}`);
    const luminance = Math.round(red * 0.2126 + green * 0.7152 + blue * 0.0722);
    minimumLuminance = Math.min(minimumLuminance, luminance);
    maximumLuminance = Math.max(maximumLuminance, luminance);
  }
  return {
    width: png.width,
    height: png.height,
    sampledColorCount: colors.size,
    luminanceRange: maximumLuminance - minimumLuminance,
    visuallyEmpty: colors.size < 4 || maximumLuminance - minimumLuminance < 6
  };
}

async function createCaptureSession(browser, profileDirectory, options) {
  const running = await startBrowser(browser, profileDirectory, options.processTimeoutMs);
  try {
    const pageTarget = await findPageTarget(running.port, options.processTimeoutMs);
    const client = await createCdpClient(pageTarget, options.processTimeoutMs);
    await client.send('Page.enable');
    await client.send('Runtime.enable');
    return {
      client,
      running,
      currentUrl: 'about:blank',
      close() {
        client.close();
        running.child.kill('SIGKILL');
      }
    };
  } catch (error) {
    running.child.kill('SIGKILL');
    throw error;
  }
}

async function captureTarget(session, target, options) {
  const { client, running } = session;
  const navigationUrl = new URL(target.url);
  if (target.viewportQuery) navigationUrl.searchParams.set(target.viewportQuery, String(target.viewport.width));
  const resolvedUrl = navigationUrl.toString();
  await client.send('Emulation.setDeviceMetricsOverride', {
    width: target.viewport.width,
    height: target.viewport.height,
    deviceScaleFactor: 1,
    mobile: target.viewport.device === 'mobile'
  });
  if (target.captureMode === 'html') {
    const response = await fetch(resolvedUrl);
    if (!response.ok) throw new Error(`Preview HTML request failed with ${response.status}.`);
    const sourceHtml = await response.text();
    const baseElement = `<base href=${JSON.stringify(resolvedUrl)}>`;
    const html = sourceHtml.includes('<head>')
      ? sourceHtml.replace('<head>', `<head>${baseElement}`)
      : `${baseElement}${sourceHtml}`;
    const frameTree = await client.send('Page.getFrameTree');
    await client.send('Page.setDocumentContent', { frameId: frameTree.frameTree.frame.id, html });
    session.currentUrl = resolvedUrl;
  } else if (session.currentUrl !== resolvedUrl) {
    await client.send('Page.navigate', { url: resolvedUrl });
    session.currentUrl = resolvedUrl;
  }
  const pageState = await waitForStudioMount(client, target.captureMode === 'html' ? undefined : resolvedUrl, options.pageReadyTimeoutMs);
  await client.send('Page.stopLoading').catch(() => undefined);
  await delay(options.settleMs);
  await client.send('Runtime.evaluate', {
    expression: 'document.fonts?.ready ?? Promise.resolve()',
    awaitPromise: true,
    returnByValue: true
  });
  const screenshot = await client.send('Page.captureScreenshot', {
    format: 'png',
    fromSurface: true,
    captureBeyondViewport: false
  });
  return {
    contents: Buffer.from(screenshot.data, 'base64'),
    pageState,
    browserDiagnostic: running.diagnostic()
      .split('\n')
      .filter((line) => line && !line.includes('CVDisplayLinkCreateWithCGDisplay'))
      .slice(-10)
      .join('\n') || undefined
  };
}

const manifestPath = argument('--manifest');
if (!manifestPath) throw new Error('Usage: npm run baseline:capture -- --manifest <run.json> [--output <directory>] [--browser <executable>] [--allow-partial]');

const outputRoot = path.resolve(argument('--output') ?? '.web-design-studio-baselines');
const browser = resolveBrowser(argument('--browser'));
const settleMs = Number(argument('--settle-ms') ?? 2500);
if (!Number.isFinite(settleMs) || settleMs < 0 || settleMs > 30000) throw new Error('--settle-ms must be between 0 and 30000.');
const pageReadyTimeoutMs = Number(argument('--page-ready-timeout-ms') ?? 30000);
if (!Number.isFinite(pageReadyTimeoutMs) || pageReadyTimeoutMs < 1000 || pageReadyTimeoutMs > 120000) {
  throw new Error('--page-ready-timeout-ms must be between 1000 and 120000.');
}
const processTimeoutMs = Number(argument('--process-timeout-ms') ?? 60000);
if (!Number.isFinite(processTimeoutMs) || processTimeoutMs < 1000 || processTimeoutMs > 60000) {
  throw new Error('--process-timeout-ms must be between 1000 and 60000.');
}

const definition = JSON.parse(await readFile(path.resolve(manifestPath), 'utf8'));
const allowPartial = process.argv.includes('--allow-partial');
const requestedViewport = argument('--viewport');
const requestedBenchmark = argument('--benchmark');
if ((requestedViewport || requestedBenchmark) && !allowPartial) throw new Error('--viewport and --benchmark are smoke-test options and require --allow-partial.');
const completePlan = createBaselineCapturePlan(definition, !allowPartial);
const plan = completePlan.filter((target) =>
  (!requestedViewport || target.viewport.id === requestedViewport)
  && (!requestedBenchmark || target.benchmarkId === requestedBenchmark));
if (plan.length === 0) throw new Error(`The requested benchmark or viewport selection is empty: ${requestedBenchmark ?? '*'} / ${requestedViewport ?? '*'}`);
const userDataDirectory = await mkdtemp(path.join(os.tmpdir(), 'web-design-baseline-chrome-'));
const results = [];
let session;

try {
  session = await createCaptureSession(browser, userDataDirectory, { processTimeoutMs });
  for (const target of plan) {
    const screenshotPath = path.join(outputRoot, target.relativeScreenshotPath);
    await mkdir(path.dirname(screenshotPath), { recursive: true });
    await rm(screenshotPath, { force: true });
    const startedAt = new Date().toISOString();
    try {
      const capture = await captureTarget(session, target, { settleMs, pageReadyTimeoutMs, processTimeoutMs });
      const analysis = analyzeScreenshot(capture.contents);
      if (analysis.width !== target.viewport.width || analysis.height !== target.viewport.height) {
        throw new Error(`Screenshot dimensions are ${analysis.width}x${analysis.height}, expected ${target.viewport.width}x${target.viewport.height}.`);
      }
      if (analysis.visuallyEmpty) throw new Error('Screenshot is visually empty or monochrome.');
      await writeFile(screenshotPath, capture.contents);
      results.push({
        benchmarkId: target.benchmarkId,
        viewportId: target.viewport.id,
        width: target.viewport.width,
        height: target.viewport.height,
        url: target.url,
        projectId: target.projectId,
        documentId: target.documentId,
        screenshot: path.relative(outputRoot, screenshotPath),
        sha256: createHash('sha256').update(capture.contents).digest('hex'),
        byteLength: capture.contents.byteLength,
        pageState: capture.pageState,
        imageAnalysis: analysis,
        browserDiagnostic: capture.browserDiagnostic,
        startedAt,
        completedAt: new Date().toISOString(),
        status: 'captured'
      });
      process.stdout.write(`captured ${target.benchmarkId} ${target.viewport.id}\n`);
    } catch (error) {
      results.push({
        benchmarkId: target.benchmarkId,
        viewportId: target.viewport.id,
        width: target.viewport.width,
        height: target.viewport.height,
        url: target.url,
        startedAt,
        completedAt: new Date().toISOString(),
        status: 'failed',
        error: error instanceof Error ? error.message : String(error)
      });
      process.stderr.write(`failed ${target.benchmarkId} ${target.viewport.id}\n`);
    }
  }
} finally {
  session?.close();
  await rm(userDataDirectory, { recursive: true, force: true });
}

const reportPath = path.join(outputRoot, definition.runId, 'capture-manifest.json');
await mkdir(path.dirname(reportPath), { recursive: true });
await writeFile(reportPath, `${JSON.stringify({
  schemaVersion: 1,
  runId: definition.runId,
  sourceVersion: definition.sourceVersion,
  browser,
  settleMs,
  pageReadyTimeoutMs,
  processTimeoutMs,
  capturedAt: new Date().toISOString(),
  expectedCaptures: plan.length,
  successfulCaptures: results.filter((result) => result.status === 'captured').length,
  failedCaptures: results.filter((result) => result.status === 'failed').length,
  results
}, null, 2)}\n`);

if (results.some((result) => result.status === 'failed')) process.exitCode = 1;
process.stdout.write(`report ${reportPath}\n`);
