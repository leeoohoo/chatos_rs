import { useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent } from 'react';
import type { SceneEditorCommand } from '../../src/v2/scene-editor-command';
import {
  createMoveSceneNodesTransaction,
  createResizeSceneNodeTransaction,
  type SceneResizeHandle
} from '../../src/v2/scene-editor-transaction';
import { renderSceneDocumentRoot } from '../../src/v2/scene-html-renderer';
import { resolveResponsiveScene } from '../../src/v2/responsive-scene';
import {
  indexSceneDocument,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneDocument,
  type SceneNode,
  type ScenePrototypeLink
} from '../../src/v2/scene-schema';
import { applySceneTransaction } from '../../src/v2/scene-transaction';
import { solveSceneLayout } from '../../src/v2/layout-engine';
import { SelectionOverlay, type SelectionOverlayItem } from './SelectionOverlay';
import {
  normalizedSelectionRect,
  selectionBounds,
  selectionNodesInRect,
  type EditorSelectableNode,
  type EditorSelectionRect
} from './selection-model';
import { LibraryCanvasComponent } from './LibraryCanvasComponent';
import type { SceneInsertionFocus } from './scene-insertion-target';

const RESIZE_HANDLES: readonly SceneResizeHandle[] = [
  'north', 'north-east', 'east', 'south-east', 'south', 'south-west', 'west', 'north-west'
];

type PointerTransform = {
  kind: 'move' | 'resize';
  pointerId: number;
  startClientX: number;
  startClientY: number;
  scale: number;
  snapshot: SceneDocument;
  nodeIds: string[];
  nodeId: string;
  handle?: SceneResizeHandle;
  deltaX: number;
  deltaY: number;
};

type PointerMarquee = {
  pointerId: number;
  startClientX: number;
  startClientY: number;
  startPoint: { x: number; y: number };
  scale: number;
  initialIds: string[];
  additive: boolean;
  moved: boolean;
};

function childrenOf(node: SceneNode): SceneNode[] {
  if (isSceneContainer(node)) return node.children;
  if (isSceneSlotContainer(node)) return Object.values(node.slots).flat();
  return [];
}

function firstVisibleFill(node: SceneNode): string | undefined {
  const paint = node.appearance.fills.find((candidate) => candidate.visible);
  if (!paint) return undefined;
  if (paint.type === 'solid') return paint.color;
  if ((paint.type === 'linear-gradient' || paint.type === 'radial-gradient') && paint.stops?.length) {
    const stops = paint.stops.map((stop) => `${stop.color} ${Math.round(stop.offset * 100)}%`).join(', ');
    return paint.type === 'linear-gradient' ? `linear-gradient(135deg, ${stops})` : `radial-gradient(circle, ${stops})`;
  }
  return undefined;
}

function sceneNodeStyle(node: SceneNode, box: { x: number; y: number; width: number; height: number }): CSSProperties {
  const typography = node.appearance.typography;
  const stroke = node.appearance.strokes.find((candidate) => candidate.paint.visible && candidate.paint.type === 'solid' && candidate.paint.color);
  const shadows = node.appearance.effects.filter((effect) => effect.visible && (effect.type === 'drop-shadow' || effect.type === 'inner-shadow'));
  return {
    position: 'absolute',
    left: box.x,
    top: box.y,
    width: box.width,
    height: box.height,
    boxSizing: 'border-box',
    opacity: node.appearance.opacity,
    display: node.visible ? undefined : 'none',
    overflow: node.layout.clipContent ? 'hidden' : 'visible',
    background: node.type === 'text' ? undefined : firstVisibleFill(node),
    color: node.type === 'text' ? firstVisibleFill(node) ?? '#18202a' : undefined,
    borderStyle: stroke?.style,
    borderColor: stroke?.paint.color,
    borderWidth: stroke ? `${stroke.width.top}px ${stroke.width.right}px ${stroke.width.bottom}px ${stroke.width.left}px` : undefined,
    borderRadius: `${node.appearance.radius.topLeft}px ${node.appearance.radius.topRight}px ${node.appearance.radius.bottomRight}px ${node.appearance.radius.bottomLeft}px`,
    boxShadow: shadows.length ? shadows.map((effect) => `${effect.type === 'inner-shadow' ? 'inset ' : ''}${effect.offset?.x ?? 0}px ${effect.offset?.y ?? 0}px ${effect.radius}px ${effect.spread ?? 0}px ${effect.color ?? 'rgba(0,0,0,.18)'}`).join(', ') : undefined,
    transform: `rotate(${node.transform.rotation}deg) scale(${node.transform.scaleX}, ${node.transform.scaleY}) skew(${node.transform.skewX}deg, ${node.transform.skewY}deg)`,
    transformOrigin: 'center',
    fontFamily: typography?.fontFamily,
    fontSize: typography?.fontSize,
    fontWeight: typography?.fontWeight,
    lineHeight: typography?.lineHeight,
    letterSpacing: typography?.letterSpacing,
    textAlign: typography?.textAlign,
    whiteSpace: node.type === 'text' ? 'pre-wrap' : undefined,
    overflowWrap: node.type === 'text' ? 'anywhere' : undefined
  };
}

function SceneNodeLayer({ node, box, interactive, contentFocused }: {
  node: SceneNode;
  box: { x: number; y: number; width: number; height: number };
  interactive: boolean;
  contentFocused: boolean;
}) {
  let content = null;
  if (node.type === 'text') content = node.content;
  else if (node.type === 'library-instance') content = <LibraryCanvasComponent component={{
    id: node.id,
    width: box.width,
    height: box.height,
    content: node.content ?? node.name,
    library: { name: node.library, component: node.component, props: node.properties }
  }} preview={interactive} />;
  else if (node.type === 'media') content = <span className="scene-v2-media-placeholder">{node.alt ?? node.mediaType}</span>;
  return <div data-scene-node-id={node.id} className={`scene-v2-node scene-${node.type} ${contentFocused ? 'scene-v2-content-focus' : ''}`} style={sceneNodeStyle(node, box)}>
    {content}
    {node.type === 'library-instance' && !interactive && <span className="scene-v2-selection-hit" aria-hidden="true" />}
    {interactive && node.prototypeLink && <button className="scene-v2-prototype-hit" aria-label={`${node.name}：打开关联画板`} />}
  </div>;
}

function selectableNodes(document: SceneDocument, rootNodeId: string, viewportWidth: number): EditorSelectableNode[] {
  const effective = resolveResponsiveScene(document, viewportWidth).document;
  const index = indexSceneDocument(effective);
  const solved = solveSceneLayout(effective, { rootNodeId, viewportWidth });
  const root = index.get(rootNodeId)?.node;
  if (!root) return [];
  const nodes: EditorSelectableNode[] = [];
  let order = 0;
  const visit = (node: SceneNode, parentId?: string) => {
    const box = solved.boxes.get(node.id);
    if (box) {
      nodes.push({
        id: node.id,
        name: node.name,
        type: node.type,
        parentId,
        zIndex: order,
        locked: node.locked,
        visible: node.visible,
        rect: { x: box.x, y: box.y, width: box.width, height: box.height }
      });
      order += 1;
    }
    for (const child of childrenOf(node)) visit(child, node.id);
  };
  visit(root);
  return nodes;
}

function pointerScale(element: HTMLElement): number {
  const bounds = element.getBoundingClientRect();
  return Math.max(0.0001, bounds.width / Math.max(1, element.offsetWidth));
}

function previewTransform(active: PointerTransform, deltaX: number, deltaY: number): SceneDocument {
  const common = { transactionId: 'preview:scene-transform', author: 'human' as const };
  const transaction = active.kind === 'move'
    ? createMoveSceneNodesTransaction(active.snapshot, { ...common, nodeIds: active.nodeIds, deltaX, deltaY })
    : createResizeSceneNodeTransaction(active.snapshot, {
      ...common,
      nodeId: active.nodeId,
      handle: active.handle!,
      deltaX,
      deltaY,
      minimumWidth: 1,
      minimumHeight: 1
    });
  return applySceneTransaction(active.snapshot, transaction, active.snapshot.updatedAt).document;
}

export function sceneArtboardContentHeight(
  scene: SceneDocument,
  pageId: string,
  viewportWidth: number,
  minimumHeight: number
): number {
  const rootNodeId = scene.pages.find((page) => page.id === pageId)?.children[0]?.id;
  if (!rootNodeId) return minimumHeight;
  try {
    return Math.max(minimumHeight, renderSceneDocumentRoot(scene, rootNodeId, viewportWidth).height);
  } catch {
    return minimumHeight;
  }
}

export function sceneArtboardSelectionBounds(
  scene: SceneDocument,
  pageId: string,
  viewportWidth: number,
  selectedIds: readonly string[]
): EditorSelectionRect | undefined {
  const rootNodeId = scene.pages.find((page) => page.id === pageId)?.children[0]?.id;
  if (!rootNodeId) return undefined;
  const selected = new Set(selectedIds);
  return selectionBounds(selectableNodes(scene, rootNodeId, viewportWidth)
    .filter((node) => selected.has(node.id))
    .map((node) => node.rect));
}

export function SceneArtboardCanvas({
  scene,
  pageId,
  viewportWidth,
  viewportHeight,
  active,
  interactive = false,
  selectionOnly = false,
  selectedIds,
  primaryId,
  onSelectionChange,
  onCommit,
  onError,
  onPrototypeActivate,
  contentFocus
}: {
  scene: SceneDocument;
  pageId: string;
  viewportWidth: number;
  viewportHeight: number;
  active: boolean;
  interactive?: boolean;
  selectionOnly?: boolean;
  selectedIds: readonly string[];
  primaryId?: string;
  onSelectionChange: (ids: string[], primaryId?: string) => void;
  onCommit: (command: SceneEditorCommand) => Promise<SceneDocument>;
  onError: (message: string) => void;
  onPrototypeActivate?: (link: ScenePrototypeLink, node: SceneNode) => void;
  contentFocus?: SceneInsertionFocus;
}) {
  const canvasRef = useRef<HTMLDivElement | null>(null);
  const [previewScene, setPreviewScene] = useState<SceneDocument>();
  const [transform, setTransform] = useState<PointerTransform>();
  const [marquee, setMarquee] = useState<PointerMarquee>();
  const [marqueeRect, setMarqueeRect] = useState<EditorSelectionRect>();
  const displayedScene = previewScene ?? scene;
  const effectiveScene = useMemo(() => resolveResponsiveScene(displayedScene, viewportWidth).document, [displayedScene, viewportWidth]);
  const page = displayedScene.pages.find((candidate) => candidate.id === pageId);
  const rootNodeId = page?.children[0]?.id;
  const nodes = useMemo(
    () => rootNodeId ? selectableNodes(displayedScene, rootNodeId, viewportWidth) : [],
    [displayedScene, rootNodeId, viewportWidth]
  );
  const nodesById = useMemo(() => new Map(nodes.map((node) => [node.id, node])), [nodes]);
  const rendered = useMemo(
    () => rootNodeId ? renderSceneDocumentRoot(displayedScene, rootNodeId, viewportWidth) : undefined,
    [displayedScene, rootNodeId, viewportWidth]
  );
  const renderedNodes = useMemo(() => {
    if (!rootNodeId) return [];
    const solved = solveSceneLayout(effectiveScene, { rootNodeId, viewportWidth });
    const effectiveIndex = indexSceneDocument(effectiveScene);
    return [...effectiveIndex.values()].flatMap(({ node, pageId: nodePageId }) => {
      const box = solved.boxes.get(node.id);
      return nodePageId === pageId && box && node.visible ? [{ node, box }] : [];
    });
  }, [displayedScene, effectiveScene, pageId, rootNodeId, viewportWidth]);
  const overlayItems = useMemo<SelectionOverlayItem[]>(() => selectedIds.flatMap((id) => {
    const node = nodesById.get(id);
    if (!node) return [];
    return [{ id, name: node.name, locked: Boolean(node.locked), primary: id === primaryId, rect: node.rect }];
  }), [nodesById, primaryId, selectedIds]);

  useEffect(() => {
    if (!transform && !marquee) return;
    const onMove = (event: PointerEvent) => {
      if (transform && event.pointerId === transform.pointerId) {
        const deltaX = Math.round(((event.clientX - transform.startClientX) / transform.scale) * 10) / 10;
        const deltaY = Math.round(((event.clientY - transform.startClientY) / transform.scale) * 10) / 10;
        const relevantX = transform.kind === 'move' || transform.handle?.includes('east') || transform.handle?.includes('west') ? deltaX : 0;
        const relevantY = transform.kind === 'move' || transform.handle?.includes('north') || transform.handle?.includes('south') ? deltaY : 0;
        setTransform((current) => current ? { ...current, deltaX: relevantX, deltaY: relevantY } : current);
        if (relevantX === 0 && relevantY === 0) {
          setPreviewScene(undefined);
          return;
        }
        try {
          setPreviewScene(previewTransform(transform, relevantX, relevantY));
        } catch (error) {
          onError(error instanceof Error ? error.message : String(error));
        }
        return;
      }
      if (marquee && event.pointerId === marquee.pointerId) {
        const moved = marquee.moved || Math.hypot(event.clientX - marquee.startClientX, event.clientY - marquee.startClientY) >= 4;
        if (!moved) return;
        const point = {
          x: marquee.startPoint.x + (event.clientX - marquee.startClientX) / marquee.scale,
          y: marquee.startPoint.y + (event.clientY - marquee.startClientY) / marquee.scale
        };
        const rect = normalizedSelectionRect(marquee.startPoint, point);
        const matched = selectionNodesInRect(nodes, rect).map((node) => node.id);
        const nextIds = marquee.additive
          ? [...marquee.initialIds, ...matched.filter((id) => !marquee.initialIds.includes(id))]
          : matched;
        setMarquee((current) => current ? { ...current, moved: true } : current);
        setMarqueeRect(rect);
        onSelectionChange(nextIds, matched.at(-1) ?? nextIds.at(-1));
      }
    };
    const onUp = (event: PointerEvent) => {
      if (transform && event.pointerId === transform.pointerId) {
        const completed = transform;
        setTransform(undefined);
        if (completed.deltaX === 0 && completed.deltaY === 0) {
          setPreviewScene(undefined);
          return;
        }
        const command: SceneEditorCommand = completed.kind === 'move'
          ? { type: 'move', nodeIds: completed.nodeIds, deltaX: completed.deltaX, deltaY: completed.deltaY }
          : {
            type: 'resize', nodeId: completed.nodeId, handle: completed.handle!,
            deltaX: completed.deltaX, deltaY: completed.deltaY, minimumWidth: 1, minimumHeight: 1
          };
        void onCommit(command).catch((error) => {
          onError(error instanceof Error ? error.message : String(error));
        }).finally(() => setPreviewScene(undefined));
        return;
      }
      if (marquee && event.pointerId === marquee.pointerId) {
        if (!marquee.moved && !marquee.additive) onSelectionChange([]);
        setMarquee(undefined);
        setMarqueeRect(undefined);
      }
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onUp);
    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointercancel', onUp);
    };
  }, [marquee, nodes, onCommit, onError, onSelectionChange, transform]);

  function beginNodeMove(event: ReactPointerEvent<HTMLDivElement>) {
    if (interactive) {
      const target = event.target instanceof Element ? event.target.closest<HTMLElement>('[data-scene-node-id]') : null;
      const node = target ? indexSceneDocument(effectiveScene).get(target.dataset.sceneNodeId!)?.node : undefined;
      if (node?.prototypeLink) {
        event.preventDefault();
        event.stopPropagation();
        onPrototypeActivate?.(node.prototypeLink, node);
      }
      return;
    }
    if (!active || event.button !== 0) return;
    const target = event.target instanceof Element ? event.target.closest<HTMLElement>('[data-scene-node-id]') : null;
    if (!target) {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const bounds = canvas.getBoundingClientRect();
      const scale = pointerScale(canvas);
      setMarquee({
        pointerId: event.pointerId,
        startClientX: event.clientX,
        startClientY: event.clientY,
        startPoint: { x: (event.clientX - bounds.left) / scale, y: (event.clientY - bounds.top) / scale },
        scale,
        initialIds: [...selectedIds],
        additive: event.shiftKey,
        moved: false
      });
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const nodeId = target.dataset.sceneNodeId!;
    const node = nodesById.get(nodeId);
    const nextIds = event.shiftKey
      ? selectedIds.includes(nodeId) ? selectedIds.filter((id) => id !== nodeId) : [...selectedIds, nodeId]
      : selectedIds.includes(nodeId) ? [...selectedIds] : [nodeId];
    onSelectionChange(nextIds, nextIds.includes(nodeId) ? nodeId : nextIds.at(-1));
    if (selectionOnly || !node || node.locked || event.shiftKey || nextIds.length === 0) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    setTransform({
      kind: 'move', pointerId: event.pointerId, startClientX: event.clientX, startClientY: event.clientY,
      scale: pointerScale(canvas), snapshot: scene, nodeIds: nextIds, nodeId, deltaX: 0, deltaY: 0
    });
  }

  function selectNodeFromClick(event: ReactMouseEvent<HTMLDivElement>) {
    if (interactive || !active) return;
    const target = event.target instanceof Element ? event.target.closest<HTMLElement>('[data-scene-node-id]') : null;
    if (!target) return;
    const nodeId = target.dataset.sceneNodeId;
    if (!nodeId || !nodesById.has(nodeId)) return;
    const nextIds = event.shiftKey
      ? selectedIds.includes(nodeId) ? selectedIds.filter((id) => id !== nodeId) : [...selectedIds, nodeId]
      : [nodeId];
    onSelectionChange(nextIds, nextIds.includes(nodeId) ? nodeId : nextIds.at(-1));
  }

  function beginResize(nodeId: string, handle: SceneResizeHandle, event: ReactPointerEvent<HTMLSpanElement>) {
    if (!active) return;
    event.preventDefault();
    event.stopPropagation();
    const canvas = canvasRef.current;
    if (!canvas) return;
    setTransform({
      kind: 'resize', pointerId: event.pointerId, startClientX: event.clientX, startClientY: event.clientY,
      scale: pointerScale(canvas), snapshot: scene, nodeIds: [nodeId], nodeId, handle, deltaX: 0, deltaY: 0
    });
  }

  if (!rootNodeId || !rendered) {
    return <div className="scene-v2-empty" style={{ minHeight: viewportHeight }}><strong>这个画板还没有 Scene 根节点</strong><span>让 AI 开始当前画板的第一个设计步骤。</span></div>;
  }

  return <div
    ref={canvasRef}
    className="scene-v2-artboard-canvas"
    style={{ width: viewportWidth, minHeight: Math.max(viewportHeight, rendered.height) }}
    onPointerDown={beginNodeMove}
    onClick={selectNodeFromClick}
  >
    <div className="scene-v2-rendered-content" style={{ position: 'relative', width: rendered.width, height: rendered.height }}>
      {renderedNodes.map(({ node, box }) => <SceneNodeLayer key={node.id} node={node} box={box} interactive={interactive} contentFocused={!interactive && contentFocus?.nodeId === node.id} />)}
    </div>
    {active && <SelectionOverlay
      items={overlayItems}
      marqueeRect={marqueeRect}
      resizeHandles={RESIZE_HANDLES}
      onResizePointerDown={beginResize}
    />}
  </div>;
}
