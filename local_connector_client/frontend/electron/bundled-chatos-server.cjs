// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');

const MIME_TYPES = new Map([
  ['.css', 'text/css; charset=utf-8'],
  ['.html', 'text/html; charset=utf-8'],
  ['.ico', 'image/x-icon'],
  ['.jpeg', 'image/jpeg'],
  ['.jpg', 'image/jpeg'],
  ['.js', 'text/javascript; charset=utf-8'],
  ['.json', 'application/json; charset=utf-8'],
  ['.map', 'application/json; charset=utf-8'],
  ['.png', 'image/png'],
  ['.svg', 'image/svg+xml'],
  ['.ttf', 'font/ttf'],
  ['.wasm', 'application/wasm'],
  ['.webp', 'image/webp'],
  ['.woff', 'font/woff'],
  ['.woff2', 'font/woff2'],
]);

function errorResponse(response, statusCode, message) {
  response.writeHead(statusCode, {
    'Cache-Control': 'no-store',
    'Content-Type': 'text/plain; charset=utf-8',
    'X-Content-Type-Options': 'nosniff',
  });
  response.end(message);
}

function resolveRequestFile(rootDir, indexPath, request) {
  let pathname;
  try {
    pathname = decodeURIComponent(new URL(request.url || '/', 'http://127.0.0.1').pathname);
  } catch {
    return { error: [400, 'Invalid request path'] };
  }
  if (pathname.includes('\0') || pathname.includes('\\')) {
    return { error: [400, 'Invalid request path'] };
  }

  const relativePath = pathname.replace(/^\/+/, '');
  const candidate = path.resolve(rootDir, relativePath || 'index.html');
  if (candidate !== rootDir && !candidate.startsWith(`${rootDir}${path.sep}`)) {
    return { error: [403, 'Request path is outside the bundled frontend'] };
  }

  try {
    const stats = fs.statSync(candidate);
    if (stats.isFile()) {
      return { filePath: candidate };
    }
  } catch {
    // SPA routes fall through to index.html below.
  }

  const acceptsHtml = String(request.headers.accept || '').includes('text/html');
  if (path.extname(pathname) || !acceptsHtml) {
    return { error: [404, 'Bundled frontend asset was not found'] };
  }
  return { filePath: indexPath };
}

function serveFile(request, response, filePath) {
  const extension = path.extname(filePath).toLowerCase();
  const stats = fs.statSync(filePath);
  const immutableAsset = filePath.includes(`${path.sep}assets${path.sep}`);
  response.writeHead(200, {
    'Cache-Control': immutableAsset ? 'public, max-age=31536000, immutable' : 'no-store',
    'Content-Length': stats.size,
    'Content-Type': MIME_TYPES.get(extension) || 'application/octet-stream',
    'Cross-Origin-Resource-Policy': 'same-origin',
    'Referrer-Policy': 'no-referrer',
    'X-Content-Type-Options': 'nosniff',
  });
  if (request.method === 'HEAD') {
    response.end();
    return;
  }
  const stream = fs.createReadStream(filePath);
  stream.on('error', () => response.destroy());
  stream.pipe(response);
}

async function startBundledChatosServer(rootDirectory) {
  const rootDir = fs.realpathSync(rootDirectory);
  const indexPath = path.join(rootDir, 'index.html');
  if (!fs.statSync(indexPath).isFile()) {
    throw new Error(`Bundled Chat OS frontend is missing index.html: ${indexPath}`);
  }

  let allowedHosts = null;
  const server = http.createServer((request, response) => {
    if (!allowedHosts?.has(String(request.headers.host || '').toLowerCase())) {
      errorResponse(response, 403, 'Invalid bundled frontend host');
      return;
    }
    if (request.method !== 'GET' && request.method !== 'HEAD') {
      errorResponse(response, 405, 'Method not allowed');
      return;
    }
    const resolved = resolveRequestFile(rootDir, indexPath, request);
    if (resolved.error) {
      errorResponse(response, resolved.error[0], resolved.error[1]);
      return;
    }
    serveFile(request, response, resolved.filePath);
  });

  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      server.off('error', reject);
      resolve();
    });
  });
  const address = server.address();
  if (!address || typeof address === 'string') {
    server.close();
    throw new Error('Bundled Chat OS frontend server did not expose a TCP address');
  }
  allowedHosts = new Set([
    `127.0.0.1:${address.port}`,
    `localhost:${address.port}`,
  ]);

  return {
    origin: `http://127.0.0.1:${address.port}`,
    close: () => new Promise((resolve) => server.close(() => resolve())),
  };
}

module.exports = {
  startBundledChatosServer,
};
