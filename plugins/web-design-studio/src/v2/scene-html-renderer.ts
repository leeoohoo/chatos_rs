import { isSceneContainer, isSceneSlotContainer, type SceneDocument, type SceneNode } from './scene-schema.js';
import { solveSceneLayout, type SceneLayoutDiagnostic, type SolvedSceneBox } from './layout-engine.js';
import { resolveResponsiveScene } from './responsive-scene.js';
import type { Phase2LayoutBenchmark } from './phase2-layout-benchmarks.js';

export interface RenderedPhase2Scene {
  benchmarkId: string;
  viewportWidth: number;
  height: number;
  nodeCount: number;
  diagnostics: SceneLayoutDiagnostic[];
  html: string;
}

export interface RenderedSceneRoot {
  documentId: string;
  revision: number;
  rootNodeId: string;
  viewportWidth: number;
  width: number;
  height: number;
  nodeCount: number;
  diagnostics: SceneLayoutDiagnostic[];
  html: string;
  documentHtml: string;
}

function escapeHtml(value: string): string {
  return value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;').replaceAll("'", '&#39;');
}

function format(value: number): string {
  return Number(value.toFixed(3)).toString();
}

function childrenOf(node: SceneNode): SceneNode[] {
  if (isSceneContainer(node)) return node.children;
  if (isSceneSlotContainer(node)) return Object.values(node.slots).flat();
  return [];
}

function flatten(nodes: SceneNode[], output: SceneNode[] = []): SceneNode[] {
  for (const node of nodes) {
    if (!node.visible) continue;
    output.push(node);
    flatten(childrenOf(node), output);
  }
  return output;
}

function firstPaintColor(node: SceneNode): string | undefined {
  return node.appearance.fills.find((paint) => paint.visible && paint.type === 'solid')?.color;
}

function firstPaintBackground(node: SceneNode): string | undefined {
  const paint = node.appearance.fills.find((candidate) => candidate.visible);
  if (!paint) return undefined;
  if (paint.type === 'solid') return paint.color;
  if ((paint.type === 'linear-gradient' || paint.type === 'radial-gradient') && paint.stops?.length) {
    const stops = paint.stops.map((stop) => `${stop.color} ${format(stop.offset * 100)}%`).join(',');
    return paint.type === 'linear-gradient'
      ? `linear-gradient(135deg,${stops})`
      : `radial-gradient(circle,${stops})`;
  }
  return undefined;
}

function componentSlug(node: Extract<SceneNode, { type: 'library-instance' }>): string {
  const configured = node.properties.componentSlug;
  if (typeof configured === 'string' && configured.trim()) return configured.trim();
  return node.component
    .replace(/([a-z0-9])([A-Z])/g, '$1-$2')
    .replace(/([A-Z])([A-Z][a-z])/g, '$1-$2')
    .toLowerCase();
}

function safeScriptJson(value: unknown): string {
  return JSON.stringify(value).replaceAll('<', '\\u003c').replaceAll('>', '\\u003e').replaceAll('&', '\\u0026');
}

function hueFor(value: string): number {
  let hash = 0;
  for (const character of value) hash = (hash * 31 + character.charCodeAt(0)) >>> 0;
  return hash % 360;
}

function nodeStyle(node: SceneNode, box: SolvedSceneBox, benchmarkId: string): string {
  const declarations = [
    'position:absolute',
    `left:${format(box.x)}px`,
    `top:${format(box.y)}px`,
    `width:${format(box.width)}px`,
    `height:${format(box.height)}px`,
    'box-sizing:border-box',
    `opacity:${node.appearance.opacity}`,
    `border-radius:${node.type === 'shape' && node.shape === 'ellipse' ? '50%' : `${format(node.appearance.radius.topLeft)}px ${format(node.appearance.radius.topRight)}px ${format(node.appearance.radius.bottomRight)}px ${format(node.appearance.radius.bottomLeft)}px`}`,
    `overflow:${node.layout.clipContent || node.type === 'library-instance' ? 'hidden' : 'visible'}`,
    `transform:rotate(${format(node.transform.rotation)}deg) scale(${format(node.transform.scaleX)},${format(node.transform.scaleY)}) skew(${format(node.transform.skewX)}deg,${format(node.transform.skewY)}deg)`,
    'transform-origin:center'
  ];
  const color = firstPaintColor(node);
  const background = firstPaintBackground(node);
  if (node.type === 'text') {
    const typography = node.appearance.typography;
    const fontSize = typography?.fontSize ?? 16;
    const configuredLineHeight = typography?.lineHeight ?? 1.4;
    const lineHeightPixels = fontSize * configuredLineHeight;
    const singleLineHug = node.layout.sizingX === 'hug' && box.height <= Math.max(lineHeightPixels, fontSize * 1.1) + 0.5;
    declarations.push(
      `color:${color ?? '#18202a'}`,
      `font-family:${JSON.stringify(typography?.fontFamily ?? 'Inter')},-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif`,
      `font-size:${format(typography?.fontSize ?? 16)}px`,
      `font-weight:${typography?.fontWeight ?? 400}`,
      `line-height:${format(configuredLineHeight)}`,
      `letter-spacing:${format(typography?.letterSpacing ?? 0)}px`,
      `text-align:${typography?.textAlign ?? 'left'}`,
      `white-space:${singleLineHug ? 'pre' : 'pre-wrap'}`,
      `overflow-wrap:${singleLineHug ? 'normal' : 'anywhere'}`,
      'overflow:visible'
    );
  } else if (node.type === 'media') {
    const hue = hueFor(`${benchmarkId}:${node.id}`);
    declarations.push(`background:linear-gradient(135deg,hsl(${hue} 42% 78%),hsl(${(hue + 48) % 360} 54% 92%))`, 'overflow:hidden');
  } else if (background) {
    declarations.push(`background:${background}`);
  }
  for (const stroke of node.appearance.strokes) {
    if (!stroke.paint.visible || stroke.paint.type !== 'solid' || !stroke.paint.color) continue;
    declarations.push(`border:${format(stroke.width.top)}px ${stroke.style} ${stroke.paint.color}`);
    break;
  }
  const shadows = node.appearance.effects.filter((effect) => effect.visible && (effect.type === 'drop-shadow' || effect.type === 'inner-shadow'));
  if (shadows.length) {
    declarations.push(`box-shadow:${shadows.map((effect) => `${effect.type === 'inner-shadow' ? 'inset ' : ''}${format(effect.offset?.x ?? 0)}px ${format(effect.offset?.y ?? 0)}px ${format(effect.radius)}px ${format(effect.spread ?? 0)}px ${effect.color ?? 'rgba(0,0,0,.18)'}`).join(',')}`);
  }
  return declarations.join(';');
}

function renderNode(node: SceneNode, box: SolvedSceneBox, benchmarkId: string): string {
  const content = node.type === 'text'
    ? escapeHtml(node.content)
    : node.type === 'library-instance'
      ? `<iframe data-library-runtime-instance="${escapeHtml(node.id)}" src="./?library-runtime=1&amp;library=${encodeURIComponent(node.library)}&amp;component=${encodeURIComponent(componentSlug(node))}&amp;instance=${encodeURIComponent(node.id)}&amp;layout=intrinsic" title="${escapeHtml(`${node.library} ${node.component}`)}" sandbox="allow-scripts allow-same-origin" tabindex="-1"></iframe>`
      : '';
  return `<div class="scene-node scene-${escapeHtml(node.type)}" data-scene-node-id="${escapeHtml(node.id)}" data-expected-x="${format(box.x)}" data-expected-y="${format(box.y)}" data-expected-width="${format(box.width)}" data-expected-height="${format(box.height)}" style="${escapeHtml(nodeStyle(node, box, benchmarkId))}">${content}</div>`;
}

function libraryRuntimeBridge(nodes: SceneNode[]): string {
  const instances = nodes.filter((node): node is Extract<SceneNode, { type: 'library-instance' }> => node.type === 'library-instance');
  if (instances.length === 0) return '';
  const descriptors = Object.fromEntries(instances.map((node) => [node.id, {
    props: { ...node.properties, componentSlug: componentSlug(node) },
    content: node.content ?? node.name
  }]));
  return `<script>(()=>{const descriptors=${safeScriptJson(descriptors)};const pending=new Set(Object.keys(descriptors));const scene=document.getElementById('phase2-scene');const finish=()=>{if(pending.size===0)scene.dataset.sceneReady='true'};addEventListener('message',(event)=>{if(event.origin!==location.origin||event.data?.source!=='web-design-library-runtime')return;const instance=String(event.data.instance??'');const descriptor=descriptors[instance];if(!descriptor)return;const frame=document.querySelector('[data-library-runtime-instance="'+CSS.escape(instance)+'"]');if(event.data.event==='request-props'||event.data.event==='ready'||event.data.event==='mounted')event.source?.postMessage({source:'web-design-studio',instance,type:'props',props:descriptor.props,content:descriptor.content},location.origin);if(event.data.event==='ready'||event.data.event==='mounted'){pending.delete(instance);if(frame)frame.dataset.libraryReady='true';finish()}if(event.data.event==='error'){if(frame)frame.dataset.libraryError=String(event.data.detail??'Component runtime failed');scene.dataset.sceneReady='error'}});finish()})()</script>`;
}

export function renderPhase2BenchmarkScene(benchmark: Phase2LayoutBenchmark, viewportWidth: number): RenderedPhase2Scene {
  const rendered = renderSceneDocumentRoot(benchmark.document, benchmark.rootNodeId, viewportWidth, benchmark.benchmarkId);
  return {
    benchmarkId: benchmark.benchmarkId,
    viewportWidth,
    height: rendered.height,
    nodeCount: rendered.nodeCount,
    diagnostics: rendered.diagnostics,
    html: rendered.html
  };
}

export function renderSceneDocumentRoot(document: SceneDocument, rootNodeId: string, viewportWidth: number, renderSeed = document.documentId): RenderedSceneRoot {
  const solved = solveSceneLayout(document, { rootNodeId, viewportWidth });
  const effective = resolveResponsiveScene(document, viewportWidth).document;
  const nodes = flatten(effective.pages.flatMap((page) => page.children));
  const renderedNodes = nodes.filter((node) => solved.boxes.has(node.id));
  const root = solved.boxes.get(rootNodeId)!;
  const html = renderedNodes
    .map((node) => renderNode(node, solved.boxes.get(node.id)!, renderSeed))
    .join('');
  const hasLibraryRuntime = renderedNodes.some((node) => node.type === 'library-instance');
  const sceneHtml = `<main id="phase2-scene" data-root-node-id="${escapeHtml(rootNodeId)}" data-scene-ready="${hasLibraryRuntime ? 'false' : 'true'}" style="position:relative;width:${format(root.width)}px;height:${format(root.height)}px;overflow:visible">${html}</main>`;
  const runtimeBridge = libraryRuntimeBridge(renderedNodes);
  return {
    documentId: document.documentId,
    revision: document.revision,
    rootNodeId,
    viewportWidth,
    width: root.width,
    height: root.height,
    nodeCount: solved.boxes.size,
    diagnostics: solved.diagnostics.map((diagnostic): SceneLayoutDiagnostic => ({ ...diagnostic })),
    html: sceneHtml,
    documentHtml: `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>html,body{margin:0;padding:0;background:#fff;overflow:auto}body{width:${format(root.width)}px;min-height:${format(root.height)}px}*{box-sizing:border-box}.scene-library-instance iframe{display:block;width:100%;height:100%;border:0;background:transparent;pointer-events:none}</style></head><body>${sceneHtml}${runtimeBridge}</body></html>`
  };
}
