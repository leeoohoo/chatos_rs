import assert from 'node:assert/strict';
import test from 'node:test';

import {
  EXTENSION_SUBPROTOCOL,
  LocalConnectorBridge,
  validateBootstrap,
  validateLoopbackEndpoint
} from '../src/bridge.js';

test('bootstrap accepts only short-lived loopback credentials', () => {
  const now = 1_000_000;
  assert.equal(validateLoopbackEndpoint('ws://127.0.0.1:39001/v1/extension'), 'ws://127.0.0.1:39001/v1/extension');
  assert.equal(validateLoopbackEndpoint('ws://[::1]:39001/v1/extension'), 'ws://[::1]:39001/v1/extension');
  for (const endpoint of [
    'ws://localhost:39001/v1/extension',
    'ws://192.0.2.1:39001/v1/extension',
    'wss://127.0.0.1:39001/v1/extension',
    'ws://user:secret@127.0.0.1:39001/v1/extension',
    'ws://127.0.0.1:39001/v1/extension?token=secret'
  ]) {
    assert.throws(() => validateLoopbackEndpoint(endpoint));
  }
  const valid = validateBootstrap(
    {
      protocol_version: '1.0',
      endpoint: 'ws://127.0.0.1:39001/v1/extension',
      token: 'short-lived-token-0123456789',
      expires_at_unix_ms: now + 60_000
    },
    now
  );
  assert.equal(valid.token, 'short-lived-token-0123456789');
  assert.throws(() =>
    validateBootstrap(
      {
        protocol_version: '1.0',
        endpoint: 'ws://127.0.0.1:39001/v1/extension',
        token: 'short-lived-token-0123456789',
        expires_at_unix_ms: now + 60 * 60 * 1000
      },
      now
    )
  );
});

test('bridge authenticates before serving connector requests', async () => {
  const native = new FakeNativePort();
  const chromeApi = {
    runtime: {
      connectNative(name) {
        assert.equal(name, 'ai.chatos.browser_bridge');
        return native;
      }
    }
  };
  const bridge = new LocalConnectorBridge({chromeApi, WebSocketImpl: FakeWebSocket});
  bridge.setRequestHandler(async (method, params) => {
    assert.equal(method, 'extension.listTargets');
    assert.deepEqual(params, {});
    return {targets: []};
  });
  await bridge.connect({pairingRequested: true});
  assert.equal(bridge.connected, true);
  assert.equal(native.lastRequest.params.pairing_requested, true);
  const socket = FakeWebSocket.instances.at(-1);
  assert.equal(socket.protocol, EXTENSION_SUBPROTOCOL);
  assert.equal(socket.sent[0].method, 'extension.authenticate');
  assert.equal(socket.sent[0].params.token, 'extension-bootstrap-token-012345');

  socket.receive({type: 'request', id: 42, method: 'extension.listTargets', params: {}});
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(socket.sent.at(-1), {type: 'response', id: 42, result: {targets: []}});
  await bridge.disconnect('test_complete');
  assert.equal(bridge.connected, false);
});

test('authenticated bridge remains connected after the bootstrap credential expires', async () => {
  const native = new FakeNativePort({credentialLifetimeMs: 20});
  const chromeApi = {
    runtime: {
      connectNative() {
        return native;
      }
    }
  };
  const bridge = new LocalConnectorBridge({chromeApi, WebSocketImpl: FakeWebSocket});
  await bridge.connect();
  assert.equal(bridge.connected, true);

  await new Promise(resolve => setTimeout(resolve, 50));

  assert.equal(bridge.connected, true);
  await bridge.disconnect('test_complete');
});

test('native host disconnect reports the Chrome runtime error immediately', async () => {
  const native = new FakeNativePort();
  const chromeApi = {
    runtime: {
      lastError: {message: 'Native messaging host not found.'},
      connectNative() {
        return native;
      }
    }
  };
  native.postMessage = () => queueMicrotask(() => native.onDisconnect.emit());
  const bridge = new LocalConnectorBridge({chromeApi, WebSocketImpl: FakeWebSocket});
  await assert.rejects(
    bridge.connect({pairingRequested: true}),
    error =>
      error.code === 'extension_unavailable' &&
      error.message === 'Native messaging host not found.'
  );
});

class FakeNativePort {
  onMessage = hook();
  onDisconnect = hook();
  lastRequest = null;

  constructor({credentialLifetimeMs = 60_000} = {}) {
    this.credentialLifetimeMs = credentialLifetimeMs;
  }

  postMessage(message) {
    this.lastRequest = message;
    queueMicrotask(() => {
      this.onMessage.emit({
        type: 'response',
        id: message.id,
        result: {
          protocol_version: '1.0',
          endpoint: 'ws://127.0.0.1:39001/v1/extension',
          token: 'extension-bootstrap-token-012345',
          expires_at_unix_ms: Date.now() + this.credentialLifetimeMs
        }
      });
    });
  }

  disconnect() {}
}

class FakeWebSocket {
  static instances = [];

  constructor(endpoint, protocol) {
    this.endpoint = endpoint;
    this.protocol = protocol;
    this.readyState = 0;
    this.sent = [];
    FakeWebSocket.instances.push(this);
    queueMicrotask(() => {
      this.readyState = 1;
      this.onopen?.();
    });
  }

  send(encoded) {
    const message = JSON.parse(encoded);
    this.sent.push(message);
    if (message.method === 'extension.authenticate') {
      queueMicrotask(() => {
        this.receive({
          type: 'response',
          id: message.id,
          result: {protocol_version: '1.0'}
        });
      });
    }
  }

  receive(message) {
    this.onmessage?.({data: JSON.stringify(message)});
  }

  close() {
    this.readyState = 3;
  }
}

function hook() {
  const listeners = new Set();
  return {
    addListener(listener) {
      listeners.add(listener);
    },
    removeListener(listener) {
      listeners.delete(listener);
    },
    emit(...args) {
      for (const listener of listeners) listener(...args);
    }
  };
}
