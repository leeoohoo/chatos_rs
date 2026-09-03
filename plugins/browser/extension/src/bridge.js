export const EXTENSION_PROTOCOL_VERSION = '1.0';
export const EXTENSION_SUBPROTOCOL = 'chatos-browser-extension.v1';
export const NATIVE_HOST_NAME = 'ai.chatos.browser_bridge';

const MAX_MESSAGE_BYTES = 8 * 1024 * 1024;
const REQUEST_TIMEOUT_MS = 5000;
const MAX_CREDENTIAL_LIFETIME_MS = 10 * 60 * 1000;

export class BridgeProtocolError extends Error {
  constructor(code, message) {
    super(message);
    this.name = 'BridgeProtocolError';
    this.code = code;
  }
}

export function validateBootstrap(result, now = Date.now()) {
  if (!result || result.protocol_version !== EXTENSION_PROTOCOL_VERSION) {
    throw new BridgeProtocolError('unsupported_by_backend', 'Unsupported bootstrap protocol');
  }
  const endpoint = validateLoopbackEndpoint(result.endpoint);
  if (
    typeof result.token !== 'string' ||
    result.token.length < 16 ||
    result.token.length > 4096 ||
    /\s/.test(result.token)
  ) {
    throw new BridgeProtocolError('invalid_request', 'Invalid bootstrap credential');
  }
  if (
    !Number.isSafeInteger(result.expires_at_unix_ms) ||
    result.expires_at_unix_ms <= now ||
    result.expires_at_unix_ms - now > MAX_CREDENTIAL_LIFETIME_MS
  ) {
    throw new BridgeProtocolError('token_expired', 'Bootstrap credential is expired or too long-lived');
  }
  return {
    endpoint,
    token: result.token,
    expiresAtUnixMs: result.expires_at_unix_ms
  };
}

export function validateLoopbackEndpoint(value) {
  let endpoint;
  try {
    endpoint = new URL(value);
  } catch {
    throw new BridgeProtocolError('invalid_request', 'Invalid Browser Bridge endpoint');
  }
  const hostname = endpoint.hostname.replace(/^\[|\]$/g, '');
  const ipv4 = hostname.split('.').map(part => Number(part));
  const isIpv4Loopback =
    ipv4.length === 4 &&
    ipv4.every(part => Number.isInteger(part) && part >= 0 && part <= 255) &&
    ipv4[0] === 127;
  const isIpv6Loopback = hostname === '::1';
  if (
    endpoint.protocol !== 'ws:' ||
    (!isIpv4Loopback && !isIpv6Loopback) ||
    endpoint.username ||
    endpoint.password ||
    endpoint.search ||
    endpoint.hash
  ) {
    throw new BridgeProtocolError(
      'invalid_request',
      'Browser Bridge endpoint must be an uncredentialed numeric loopback WebSocket URL'
    );
  }
  return endpoint.toString();
}

export class LocalConnectorBridge {
  #chrome;
  #WebSocket;
  #nativePort = null;
  #socket = null;
  #pending = new Map();
  #requestHandler = null;
  #stateListeners = new Set();
  #nextId = 1;
  #connected = false;
  #connecting = null;

  constructor({chromeApi = globalThis.chrome, WebSocketImpl = globalThis.WebSocket} = {}) {
    this.#chrome = chromeApi;
    this.#WebSocket = WebSocketImpl;
  }

  get connected() {
    return this.#connected;
  }

  setRequestHandler(handler) {
    this.#requestHandler = handler;
  }

  onStateChange(listener) {
    this.#stateListeners.add(listener);
    return () => this.#stateListeners.delete(listener);
  }

  async connect({pairingRequested = false} = {}) {
    if (this.#connected) return;
    if (this.#connecting) return this.#connecting;
    this.#connecting = this.#connect(pairingRequested)
      .catch(async error => {
        await this.disconnect('connection_failed');
        throw error;
      })
      .finally(() => {
        this.#connecting = null;
      });
    return this.#connecting;
  }

  async #connect(pairingRequested) {
    await this.disconnect('reconnecting');
    let nativePort;
    try {
      nativePort = this.#chrome.runtime.connectNative(NATIVE_HOST_NAME);
    } catch {
      throw new BridgeProtocolError('extension_unavailable', 'Browser MCP native host is unavailable');
    }
    this.#nativePort = nativePort;
    nativePort.onDisconnect.addListener(() => {
      if (this.#nativePort === nativePort) void this.disconnect('native_host_disconnected');
    });
    const bootstrapResult = await this.#nativeRequest(nativePort, 'extension.bootstrap', {
      protocol_version: EXTENSION_PROTOCOL_VERSION,
      pairing_requested: Boolean(pairingRequested)
    });
    const bootstrap = validateBootstrap(bootstrapResult);

    const socket = new this.#WebSocket(bootstrap.endpoint, EXTENSION_SUBPROTOCOL);
    this.#socket = socket;
    socket.onmessage = event => this.#handleSocketMessage(event.data);
    socket.onclose = () => {
      if (this.#socket === socket) void this.disconnect('bridge_disconnected');
    };
    socket.onerror = () => {};
    await waitForSocketOpen(socket);
    if (socket.protocol !== EXTENSION_SUBPROTOCOL) {
      throw new BridgeProtocolError('unsupported_by_backend', 'Browser Bridge subprotocol mismatch');
    }
    const hello = await this.#socketRequest('extension.authenticate', {
      protocol_version: EXTENSION_PROTOCOL_VERSION,
      token: bootstrap.token
    });
    if (!hello || hello.protocol_version !== EXTENSION_PROTOCOL_VERSION) {
      throw new BridgeProtocolError('unsupported_by_backend', 'Browser Bridge protocol mismatch');
    }
    this.#connected = true;
    this.#emitState();
  }

  async disconnect(reason = 'disconnected') {
    const socket = this.#socket;
    const nativePort = this.#nativePort;
    this.#socket = null;
    this.#nativePort = null;
    this.#connected = false;
    for (const pending of this.#pending.values()) {
      clearTimeout(pending.timeout);
      pending.reject(new BridgeProtocolError('extension_unavailable', reason));
    }
    this.#pending.clear();
    if (socket && socket.readyState < 2) socket.close(1000, 'closed');
    try {
      nativePort?.disconnect();
    } catch {
      // Native host may already be disconnected.
    }
    this.#emitState(reason);
  }

  notify(method, params = {}) {
    if (!this.#connected) return false;
    this.#sendWire({type: 'event', method, params});
    return true;
  }

  async #nativeRequest(port, method, params) {
    const id = `native_${crypto.randomUUID()}`;
    return new Promise((resolve, reject) => {
      const cleanup = () => {
        clearTimeout(timeout);
        port.onMessage.removeListener(messageListener);
        port.onDisconnect.removeListener(disconnectListener);
      };
      const timeout = setTimeout(() => {
        cleanup();
        reject(new BridgeProtocolError('timeout', `${method} timed out`));
      }, REQUEST_TIMEOUT_MS);
      const messageListener = message => {
        if (!message || message.type !== 'response' || message.id !== id) return;
        cleanup();
        if (message.error) {
          reject(remoteError(message.error));
        } else {
          resolve(message.result);
        }
      };
      const disconnectListener = () => {
        const message = this.#chrome.runtime.lastError?.message;
        cleanup();
        reject(
          new BridgeProtocolError(
            'extension_unavailable',
            message || 'Browser MCP native host disconnected'
          )
        );
      };
      port.onMessage.addListener(messageListener);
      port.onDisconnect.addListener(disconnectListener);
      try {
        port.postMessage({type: 'request', id, method, params});
      } catch {
        cleanup();
        reject(
          new BridgeProtocolError(
            'extension_unavailable',
            'Could not send a request to the Browser MCP native host'
          )
        );
      }
    });
  }

  async #socketRequest(method, params) {
    const id = this.#nextId++;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.#pending.delete(id);
        reject(new BridgeProtocolError('timeout', `${method} timed out`));
      }, REQUEST_TIMEOUT_MS);
      this.#pending.set(id, {resolve, reject, timeout});
      try {
        this.#sendWire({type: 'request', id, method, params});
      } catch (error) {
        clearTimeout(timeout);
        this.#pending.delete(id);
        reject(error);
      }
    });
  }

  #handleSocketMessage(data) {
    if (typeof data !== 'string' || utf8Length(data) > MAX_MESSAGE_BYTES) {
      void this.disconnect('invalid_bridge_message');
      return;
    }
    let message;
    try {
      message = JSON.parse(data);
    } catch {
      void this.disconnect('invalid_bridge_message');
      return;
    }
    if (message.type === 'response' && Number.isSafeInteger(message.id)) {
      const pending = this.#pending.get(message.id);
      if (!pending) return;
      clearTimeout(pending.timeout);
      this.#pending.delete(message.id);
      if (message.error) pending.reject(remoteError(message.error));
      else pending.resolve(message.result);
      return;
    }
    if (message.type === 'request' && Number.isSafeInteger(message.id) && typeof message.method === 'string') {
      void this.#handleRequest(message);
      return;
    }
    if (message.type === 'event' && message.method === 'bridge.disconnected') {
      void this.disconnect('bridge_disconnected');
      return;
    }
    void this.disconnect('invalid_bridge_message');
  }

  async #handleRequest(message) {
    try {
      if (!this.#connected || !this.#requestHandler) {
        throw new BridgeProtocolError('extension_unavailable', 'Extension is not ready');
      }
      const result = await this.#requestHandler(message.method, message.params ?? {});
      this.#sendWire({type: 'response', id: message.id, result: result ?? {}});
    } catch (error) {
      const protocolError = normalizeError(error);
      this.#sendWire({
        type: 'response',
        id: message.id,
        error: {code: protocolError.code, message: protocolError.message.slice(0, 1024)}
      });
    }
  }

  #sendWire(message) {
    if (!this.#socket || this.#socket.readyState !== 1) {
      throw new BridgeProtocolError('extension_unavailable', 'Browser Bridge is unavailable');
    }
    const encoded = JSON.stringify(message);
    if (utf8Length(encoded) > MAX_MESSAGE_BYTES) {
      throw new BridgeProtocolError('invalid_request', 'Browser Bridge message exceeds 8 MiB');
    }
    this.#socket.send(encoded);
  }

  #emitState(reason = null) {
    const state = {connected: this.#connected, reason};
    for (const listener of this.#stateListeners) listener(state);
  }
}

function waitForSocketOpen(socket) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new BridgeProtocolError('timeout', 'WebSocket connection timed out')), REQUEST_TIMEOUT_MS);
    socket.onopen = () => {
      clearTimeout(timeout);
      resolve();
    };
    const previousClose = socket.onclose;
    socket.onclose = event => {
      clearTimeout(timeout);
      previousClose?.(event);
      reject(new BridgeProtocolError('extension_unavailable', 'WebSocket connection closed'));
    };
  });
}

function remoteError(error) {
  const code = typeof error?.code === 'string' ? error.code : 'backend_error';
  const message = typeof error?.message === 'string' ? error.message.slice(0, 1024) : code;
  return new BridgeProtocolError(code, message);
}

function normalizeError(error) {
  if (error instanceof BridgeProtocolError) return error;
  return new BridgeProtocolError('backend_error', error instanceof Error ? error.message : 'Unknown extension error');
}

function utf8Length(value) {
  if (value.length > MAX_MESSAGE_BYTES) return value.length;
  return new TextEncoder().encode(value).byteLength;
}
