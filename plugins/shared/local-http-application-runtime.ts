import { promises as fs, watch, type FSWatcher } from 'node:fs';

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
  return { host, port };
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
