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
    `border-radius:${format(node.appearance.radius.topLeft)}px ${format(node.appearance.radius.topRight)}px ${format(node.appearance.radius.bottomRight)}px ${format(node.appearance.radius.bottomLeft)}px`
  ];
  const color = firstPaintColor(node);
  if (node.type === 'text') {
    const typography = node.appearance.typography;
    const fontSize = typography?.fontSize ?? 16;
    const configuredLineHeight = typography?.lineHeight ?? 1.4;
    const lineHeightPixels = configuredLineHeight <= 4 ? fontSize * configuredLineHeight : configuredLineHeight;
    const singleLineHug = node.layout.sizingX === 'hug' && box.height <= Math.max(lineHeightPixels, fontSize * 1.1) + 0.5;
    declarations.push(
      `color:${color ?? '#18202a'}`,
      `font-family:${JSON.stringify(typography?.fontFamily ?? 'Inter')},-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif`,
      `font-size:${format(typography?.fontSize ?? 16)}px`,
      `font-weight:${typography?.fontWeight ?? 400}`,
      `line-height:${typography?.lineHeight ?? 1.4}`,
      `letter-spacing:${format(typography?.letterSpacing ?? 0)}px`,
      `text-align:${typography?.textAlign ?? 'left'}`,
      `white-space:${singleLineHug ? 'pre' : 'pre-wrap'}`,
      `overflow-wrap:${singleLineHug ? 'normal' : 'anywhere'}`,
      'overflow:visible'
    );
  } else if (node.type === 'media') {
    const hue = hueFor(`${benchmarkId}:${node.id}`);
    declarations.push(`background:linear-gradient(135deg,hsl(${hue} 42% 78%),hsl(${(hue + 48) % 360} 54% 92%))`, 'overflow:hidden');
  } else if (color) {
    declarations.push(`background:${color}`);
  }
  for (const stroke of node.appearance.strokes) {
    if (!stroke.paint.visible || stroke.paint.type !== 'solid' || !stroke.paint.color) continue;
    declarations.push(`border:${format(stroke.width.top)}px ${stroke.style} ${stroke.paint.color}`);
    break;
  }
  return declarations.join(';');
}

function renderNode(node: SceneNode, box: SolvedSceneBox, benchmarkId: string): string {
  const content = node.type === 'text' ? escapeHtml(node.content) : '';
  return `<div class="scene-node scene-${escapeHtml(node.type)}" data-scene-node-id="${escapeHtml(node.id)}" data-expected-x="${format(box.x)}" data-expected-y="${format(box.y)}" data-expected-width="${format(box.width)}" data-expected-height="${format(box.height)}" style="${escapeHtml(nodeStyle(node, box, benchmarkId))}">${content}</div>`;
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
  const root = solved.boxes.get(rootNodeId)!;
  const html = nodes
    .filter((node) => solved.boxes.has(node.id))
    .map((node) => renderNode(node, solved.boxes.get(node.id)!, renderSeed))
    .join('');
  const sceneHtml = `<main id="phase2-scene" data-root-node-id="${escapeHtml(rootNodeId)}" data-scene-ready="true" style="position:relative;width:${format(root.width)}px;height:${format(root.height)}px;overflow:visible">${html}</main>`;
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
    documentHtml: `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>html,body{margin:0;padding:0;background:#fff;overflow:auto}body{width:${format(root.width)}px;min-height:${format(root.height)}px}*{box-sizing:border-box}</style></head><body>${sceneHtml}</body></html>`
  };
}
