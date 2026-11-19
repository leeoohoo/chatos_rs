import { createHmac, timingSafeEqual } from 'node:crypto';
import { promises as fs, watch, type FSWatcher } from 'node:fs';
import type { IncomingHttpHeaders } from 'node:http';

const authenticationHeaderName = 'x-chatos-plugin-token';
const healthChallengeHeaderName = 'x-chatos-plugin-challenge';
const healthProofHeaderName = 'X-ChatOS-Plugin-Proof';
const sessionCookieName = 'CHATOS_PLUGIN_APP_SESSION';

export type PluginDirectoryWatchMode = 'native' | 'polling';

export interface PluginLocalHttpEndpointOptions {
  defaultPort: number;
  legacyHostEnvironmentKey?: string;
  legacyPortEnvironmentKey?: string;
  environment?: NodeJS.ProcessEnv;
}

export interface PluginLocalHttpEndpoint {
  host: string;
  port: number;
  healthPath: string;
  authenticationToken?: string;
}

interface PluginLocalHttpRequest {
  headers: IncomingHttpHeaders;
  path?: string;
  url?: string;
}

interface PluginLocalHttpResponse {
  setHeader(name: string, value: string): unknown;
  sendStatus(status: number): unknown;
}

interface PluginJsonResponse {
  setHeader(name: string, value: string): unknown;
  json(value: unknown): unknown;
}

interface PluginEventRequest {
  on(event: 'close', callback: () => void): unknown;
}

interface PluginEventResponse extends PluginJsonResponse {
  flushHeaders(): unknown;
  write(value: string): unknown;
}

export interface PluginDirectoryMonitorOptions {
  directory: string;
  accepts: (fileName: string) => boolean;
  onChange: () => void;
  mode?: PluginDirectoryWatchMode;
  pollIntervalMs?: number;
}

export interface PluginDirectoryMonitor {
  close(): void;
}

export function pluginHealthHandler(service: string, dataDirectory: string) {
  return (_request: unknown, response: PluginJsonResponse): void => {
    response.setHeader('Cache-Control', 'no-store');
    response.json({ ok: true, service, dataDirectory });
  };
}

export function pluginContextHandler(context: unknown) {
  return (_request: unknown, response: PluginJsonResponse): void => {
    response.setHeader('Cache-Control', 'no-store');
    response.json(context);
  };
}

export function createPluginChangeStream(eventName: string) {
  const subscribers = new Set<PluginEventResponse>();
  return {
    publish(): void {
      const payload = `event: ${eventName}\ndata: ${JSON.stringify({ changedAt: new Date().toISOString() })}\n\n`;
      for (const response of subscribers) response.write(payload);
    },
    handle(request: PluginEventRequest, response: PluginEventResponse): void {
      response.setHeader('Content-Type', 'text/event-stream');
      response.setHeader('Cache-Control', 'no-store');
      response.setHeader('Connection', 'keep-alive');
      response.flushHeaders();
      response.write('event: ready\ndata: {}\n\n');
      subscribers.add(response);
      request.on('close', () => subscribers.delete(response));
    }
  };
}

export function setPluginStaticAssetHeaders(
  response: Pick<PluginLocalHttpResponse, 'setHeader'>,
  noStore = false
): void {
  if (noStore) {
    response.setHeader('Cache-Control', 'no-store, no-cache, must-revalidate');
    response.setHeader('Pragma', 'no-cache');
    response.setHeader('Expires', '0');
  }
  response.setHeader('X-Content-Type-Options', 'nosniff');
  response.setHeader('Referrer-Policy', 'no-referrer');
}

export function reportPluginApplicationStarted(
  name: string,
  endpoint: Pick<PluginLocalHttpEndpoint, 'host' | 'port'>
): void {
  process.stdout.write(`${name} is available at http://${endpoint.host}:${endpoint.port}\n`);
}

export function readPluginLocalHttpEndpoint(
  options: PluginLocalHttpEndpointOptions
): PluginLocalHttpEndpoint {
  const environment = options.environment ?? process.env;
  const legacyHost = options.legacyHostEnvironmentKey
    ? environment[options.legacyHostEnvironmentKey]
    : undefined;
  const legacyPort = options.legacyPortEnvironmentKey
    ? environment[options.legacyPortEnvironmentKey]
    : undefined;
  const host = environment.CHATOS_PLUGIN_APP_HOST?.trim() || legacyHost?.trim() || '127.0.0.1';
  const rawPort = environment.CHATOS_PLUGIN_APP_PORT ?? legacyPort ?? String(options.defaultPort);
  const port = Number(rawPort);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    throw new Error(`Invalid local_http application port: ${rawPort}`);
  }
  return {
    host,
    port,
    healthPath: environment.CHATOS_PLUGIN_APP_HEALTH_PATH?.trim() || '/api/health',
    ...(environment.CHATOS_PLUGIN_APP_TOKEN
      ? { authenticationToken: environment.CHATOS_PLUGIN_APP_TOKEN }
      : {})
  };
}

export function pluginLocalHttpAuthentication(endpoint: PluginLocalHttpEndpoint) {
  return (
    request: PluginLocalHttpRequest,
    response: PluginLocalHttpResponse,
    next: () => void
  ): void => {
    const token = endpoint.authenticationToken;
    if (!token) {
      next();
      return;
    }
    const requestPath = request.path ?? new URL(request.url ?? '/', 'http://127.0.0.1').pathname;
    const challenge = singleHeader(request.headers[healthChallengeHeaderName]);
    if (requestPath === endpoint.healthPath && challenge && /^[A-Za-z0-9_-]{32,128}$/.test(challenge)) {
      response.setHeader(
        healthProofHeaderName,
        createHmac('sha256', token).update(challenge).digest('base64url')
      );
      next();
      return;
    }
    const headerToken = singleHeader(request.headers[authenticationHeaderName]);
    const cookieToken = cookieValue(singleHeader(request.headers.cookie), sessionCookieName);
    if (!safeEqual(headerToken, token) && !safeEqual(cookieToken, token)) {
      response.sendStatus(401);
      return;
    }
    if (safeEqual(headerToken, token)) {
      response.setHeader(
        'Set-Cookie',
        `${sessionCookieName}=${token}; HttpOnly; SameSite=Strict; Path=/`
      );
    }
    next();
  };
}

function singleHeader(value: string | string[] | undefined): string | undefined {
  return Array.isArray(value) ? value[0] : value;
}

function cookieValue(header: string | undefined, name: string): string | undefined {
  return header?.split(';')
    .map((part) => part.trim().split('='))
    .find(([key]) => key === name)?.slice(1).join('=');
}

function safeEqual(value: string | undefined, expected: string): boolean {
  if (value === undefined) return false;
  const left = Buffer.from(value);
  const right = Buffer.from(expected);
  return left.length === right.length && timingSafeEqual(left, right);
}

export function pluginDirectoryWatchMode(
  environment: NodeJS.ProcessEnv = process.env
): PluginDirectoryWatchMode {
  return environment.CHATOS_PLUGIN_FILE_WATCH_MODE === 'polling' ? 'polling' : 'native';
}

export function startPluginDirectoryMonitor(
  options: PluginDirectoryMonitorOptions
): PluginDirectoryMonitor {
  const mode = options.mode ?? pluginDirectoryWatchMode();
  const pollIntervalMs = Math.max(250, options.pollIntervalMs ?? 750);
  let closed = false;
  let watcher: FSWatcher | undefined;
  let pollTimer: NodeJS.Timeout | undefined;
  let pollInFlight = false;
  let previousDirectoryRevision: bigint | undefined;

  const poll = async () => {
    if (closed || pollInFlight) return;
    pollInFlight = true;
    try {
      const revision = (await fs.stat(options.directory, { bigint: true })).mtimeNs;
      if (previousDirectoryRevision !== undefined && revision !== previousDirectoryRevision) {
        options.onChange();
      }
      previousDirectoryRevision = revision;
    } catch {
      // The owning store creates the directory. A transient missing/unreadable
      // directory is retried on the next bounded polling interval.
    } finally {
      pollInFlight = false;
    }
  };

  const startPolling = () => {
    if (closed || pollTimer !== undefined) return;
    void poll();
    pollTimer = setInterval(() => { void poll(); }, pollIntervalMs);
    pollTimer.unref();
  };

  if (mode === 'polling') {
    startPolling();
  } else {
    try {
      watcher = watch(options.directory, { persistent: false }, (_event, fileName) => {
        if (fileName === null || options.accepts(String(fileName))) options.onChange();
      });
      watcher.once('error', () => {
        watcher?.close();
        watcher = undefined;
        startPolling();
      });
    } catch {
      startPolling();
    }
  }

  return {
    close() {
      closed = true;
      watcher?.close();
      if (pollTimer !== undefined) clearInterval(pollTimer);
    }
  };
}
