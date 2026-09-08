import http from 'node:http';
import { createPhase2LayoutBenchmarks } from '../dist/v2-phase2-layout-benchmarks.test.mjs';
import { renderPhase2BenchmarkScene } from '../dist/v2-scene-html-renderer.test.mjs';

function argument(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

const port = Number(argument('--port') ?? 4290);
const host = argument('--host') ?? '127.0.0.1';
if (!Number.isSafeInteger(port) || port < 1024 || port > 65535) throw new Error('--port must be between 1024 and 65535.');
const benchmarks = new Map(createPhase2LayoutBenchmarks().map((benchmark) => [benchmark.benchmarkId, benchmark]));

function shell(benchmarkId) {
  return `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${benchmarkId}</title><style>html,body{margin:0;min-height:100%;background:#fbfcfd}body{overflow-x:hidden}#root{position:relative;min-height:100vh}.scene-media::after{content:"";position:absolute;inset:12%;border:1px solid rgba(255,255,255,.56);border-radius:18px;box-shadow:0 24px 80px rgba(22,31,45,.14)}@media(prefers-reduced-motion:no-preference){#phase2-scene{animation:scene-ready .12s ease-out}@keyframes scene-ready{from{opacity:.98}to{opacity:1}}}</style></head><body><div id="root"></div><script>(async()=>{const response=await fetch('/render/${benchmarkId}?width='+Math.round(innerWidth),{cache:'no-store'});if(!response.ok)throw new Error(await response.text());document.querySelector('#root').innerHTML=await response.text();document.documentElement.dataset.phase2Ready='true';})().catch(error=>{document.body.textContent=error.stack||String(error);document.documentElement.dataset.phase2Ready='error';});</script></body></html>`;
}

function fullPage(benchmark, width) {
  const rendered = renderPhase2BenchmarkScene(benchmark, width);
  return `<!doctype html><html data-phase2-ready="true"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${benchmark.benchmarkId}</title><style>html,body{margin:0;min-height:100%;background:#fbfcfd}body{overflow-x:hidden}#root{position:relative;min-height:100vh}.scene-media::after{content:"";position:absolute;inset:12%;border:1px solid rgba(255,255,255,.56);border-radius:18px;box-shadow:0 24px 80px rgba(22,31,45,.14)}</style></head><body><div id="root">${rendered.html}</div></body></html>`;
}

const server = http.createServer((request, response) => {
  try {
    const requestUrl = new URL(request.url ?? '/', `http://${request.headers.host ?? `${host}:${port}`}`);
    response.setHeader('Cache-Control', 'no-store');
    response.setHeader('X-Content-Type-Options', 'nosniff');
    if (requestUrl.pathname === '/health') {
      response.setHeader('Content-Type', 'application/json; charset=utf-8');
      response.end(JSON.stringify({ ok: true, benchmarkCount: benchmarks.size }));
      return;
    }
    const renderMatch = requestUrl.pathname.match(/^\/render\/([a-z0-9._-]+)$/i);
    const pageMatch = requestUrl.pathname.match(/^\/benchmark\/([a-z0-9._-]+)$/i);
    const benchmarkId = renderMatch?.[1] ?? pageMatch?.[1];
    const benchmark = benchmarkId ? benchmarks.get(benchmarkId) : undefined;
    if (!benchmark) {
      response.statusCode = 404;
      response.end('Unknown phase 2 benchmark');
      return;
    }
    if (renderMatch) {
      const width = Number(requestUrl.searchParams.get('width'));
      if (!Number.isFinite(width) || width <= 0 || width > 10000) throw new Error('width must be between 1 and 10000 CSS pixels.');
      const rendered = renderPhase2BenchmarkScene(benchmark, width);
      response.setHeader('Content-Type', 'text/html; charset=utf-8');
      response.setHeader('X-Scene-Node-Count', String(rendered.nodeCount));
      response.end(rendered.html);
      return;
    }
    response.setHeader('Content-Type', 'text/html; charset=utf-8');
    const requestedWidth = requestUrl.searchParams.get('width');
    if (requestedWidth !== null) {
      const width = Number(requestedWidth);
      if (!Number.isFinite(width) || width <= 0 || width > 10000) throw new Error('width must be between 1 and 10000 CSS pixels.');
      response.end(fullPage(benchmark, width));
    } else {
      response.end(shell(benchmarkId));
    }
  } catch (error) {
    response.statusCode = 500;
    response.setHeader('Content-Type', 'text/plain; charset=utf-8');
    response.end(error instanceof Error ? error.message : String(error));
  }
});

server.listen(port, host, () => {
  process.stdout.write(`Web Design Studio phase 2 preview is available at http://${host}:${port}\n`);
});
