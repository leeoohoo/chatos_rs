import {
  DEFAULT_WEB_DESIGN_BREAKPOINTS,
  pageIdForComponent,
  type WebComponentResponsiveOverride,
  type WebComponentStyle,
  type WebDesignBreakpoint,
  type WebDesignComponent,
  type WebDesignDevice,
  type WebDesignDocument,
  type WebDesignSymbol,
  type WebSymbolOverride
} from './schema.js';

export interface ResolvedWebDesignComponent {
  x: number;
  y: number;
  width: number;
  height: number;
  hidden: boolean;
  style: WebComponentStyle;
}

export interface FlattenedWebDesignComponent {
  component: WebDesignComponent;
  depth: number;
}

export interface SnapGuides {
  x?: number;
  y?: number;
}

export interface SnapResult {
  frame: ResolvedWebDesignComponent;
  guides: SnapGuides;
}

export interface ClonedComponentSubtrees {
  components: WebDesignComponent[];
  rootIds: string[];
}

export function createSymbolFromSelection(
  document: WebDesignDocument,
  componentIds: string[],
  name: string
): WebDesignSymbol {
  const roots = selectedRootIds(document, componentIds);
  if (roots.length === 0) throw new Error('Select at least one component to create a reusable component.');
  const copiedIds = new Set(roots.flatMap((id) => [id, ...descendantIds(document, id)]));
  const id = `symbol-${globalThis.crypto.randomUUID().slice(0, 8)}`;
  const components = document.components.filter((component) => copiedIds.has(component.id)).map((component) => ({
    ...structuredClone(component),
    pageId: undefined,
    parentId: component.parentId && copiedIds.has(component.parentId) ? component.parentId : undefined,
    slot: component.parentId && copiedIds.has(component.parentId) ? component.slot : undefined,
    symbolId: id,
    symbolInstanceId: undefined,
    symbolComponentId: undefined,
    symbolOverrides: undefined,
    annotations: []
  }));
  return { id, name: name.trim() || '可复用组件', rootIds: [...roots], components, createdAt: new Date().toISOString() };
}

export function instantiateSymbol(
  document: WebDesignDocument,
  symbol: WebDesignSymbol,
  targetPageId: string
): ClonedComponentSubtrees {
  const idMap = new Map(symbol.components.map((component) => [component.id, `${component.type}-${globalThis.crypto.randomUUID().slice(0, 8)}`]));
  const symbolInstanceId = `instance-${globalThis.crypto.randomUUID().slice(0, 8)}`;
  const maxZ = Math.max(0, ...componentsForPage(document, targetPageId).map((component) => component.zIndex));
  const zRanks = new Map([...symbol.components].sort((left, right) => left.zIndex - right.zIndex).map((component, index) => [component.id, index + 1]));
  const minimums = Object.fromEntries((['desktop', 'tablet', 'mobile'] as const).map((device) => {
    const frames = symbol.components.map((component) => resolveComponent(component, device));
    return [device, { x: Math.min(...frames.map((frame) => frame.x)), y: Math.min(...frames.map((frame) => frame.y)) }];
  })) as Record<WebDesignDevice, { x: number; y: number }>;
  const originX: Record<WebDesignDevice, number> = { desktop: 80, tablet: 40, mobile: 20 };
  const origins = Object.fromEntries((['desktop', 'tablet', 'mobile'] as const).map((device) => {
    const existing = componentsForPage(document, targetPageId).map((component) => resolveComponent(component, device)).filter((frame) => !frame.hidden);
    const bottom = existing.length ? Math.max(...existing.map((frame) => frame.y + frame.height)) : 40;
    return [device, { x: originX[device], y: Math.max(device === 'desktop' ? 80 : 40, bottom + 40) }];
  })) as Record<WebDesignDevice, { x: number; y: number }>;
  const components = symbol.components.map((component) => {
    let clone = structuredClone(component);
    clone.id = idMap.get(component.id)!;
    clone.name = component.name;
    clone.pageId = targetPageId;
    clone.parentId = component.parentId ? idMap.get(component.parentId) : undefined;
    clone.symbolId = symbol.id;
    clone.symbolInstanceId = symbolInstanceId;
    clone.symbolComponentId = component.id;
    clone.symbolOverrides = [];
    clone.zIndex = maxZ + zRanks.get(component.id)!;
    clone.annotations = [];
    clone.x = component.x - minimums.desktop.x + origins.desktop.x;
    clone.y = component.y - minimums.desktop.y + origins.desktop.y;
    for (const device of ['tablet', 'mobile'] as const) {
      const frame = resolveComponent(component, device);
      clone = updateComponentFrame(clone, device, {
        x: frame.x - minimums[device].x + origins[device].x,
        y: frame.y - minimums[device].y + origins[device].y,
        width: frame.width,
        height: frame.height,
        hidden: frame.hidden
      });
    }
    return clone;
  });
  return { components, rootIds: symbol.rootIds.map((id) => idMap.get(id)!) };
}

function minimumFrame(components: WebDesignComponent[], device: WebDesignDevice): { x: number; y: number } {
  const frames = components.map((component) => resolveComponent(component, device));
  return { x: Math.min(...frames.map((frame) => frame.x)), y: Math.min(...frames.map((frame) => frame.y)) };
}

function synchronizeInstanceComponent(
  component: WebDesignComponent,
  definition: WebDesignComponent,
  definitionOrigin: Record<WebDesignDevice, { x: number; y: number }>,
  instanceOrigin: Record<WebDesignDevice, { x: number; y: number }>
): WebDesignComponent {
  const overrides = new Set(component.symbolOverrides ?? []);
  let next = structuredClone(component);
  if (!overrides.has('content')) {
    next.name = definition.name;
    next.content = definition.content;
    next.interaction = structuredClone(definition.interaction);
  }
  if (!overrides.has('frame')) {
    next.layout = structuredClone(definition.layout);
    for (const device of ['desktop', 'tablet', 'mobile'] as const) {
      const frame = resolveComponent(definition, device);
      next = updateComponentFrame(next, device, {
        x: frame.x - definitionOrigin[device].x + instanceOrigin[device].x,
        y: frame.y - definitionOrigin[device].y + instanceOrigin[device].y,
        width: frame.width,
        height: frame.height,
        hidden: frame.hidden
      });
    }
  }
  if (!overrides.has('style')) {
    next.style = structuredClone(definition.style);
    for (const device of ['tablet', 'mobile'] as const) {
      const currentOverride = next.responsive?.[device];
      const definitionStyle = definition.responsive?.[device]?.style;
      if (currentOverride) {
        next.responsive = {
          ...next.responsive,
          [device]: { ...currentOverride, style: definitionStyle ? structuredClone(definitionStyle) : undefined }
        };
      }
    }
  }
  return next;
}

export function syncSymbolInstances(document: WebDesignDocument, symbolId: string): WebDesignDocument {
  const symbol = document.symbols?.find((candidate) => candidate.id === symbolId);
  if (!symbol) throw new Error(`Reusable component not found: ${symbolId}`);
  const definitionOrigin = Object.fromEntries((['desktop', 'tablet', 'mobile'] as const).map((device) => [device, minimumFrame(symbol.components, device)])) as Record<WebDesignDevice, { x: number; y: number }>;
  const instances = new Map<string, WebDesignComponent[]>();
  for (const component of document.components) {
    if (component.symbolId !== symbolId || !component.symbolInstanceId || !component.symbolComponentId) continue;
    const group = instances.get(component.symbolInstanceId) ?? [];
    group.push(component);
    instances.set(component.symbolInstanceId, group);
  }
  const synchronized = new Map<string, WebDesignComponent>();
  for (const components of instances.values()) {
    const instanceOrigin = Object.fromEntries((['desktop', 'tablet', 'mobile'] as const).map((device) => [device, minimumFrame(components, device)])) as Record<WebDesignDevice, { x: number; y: number }>;
    for (const component of components) {
      const definition = symbol.components.find((candidate) => candidate.id === component.symbolComponentId);
      if (definition) synchronized.set(component.id, synchronizeInstanceComponent(component, definition, definitionOrigin, instanceOrigin));
    }
  }
  return {
    ...document,
    components: document.components.map((component) => synchronized.get(component.id) ?? component)
  };
}

export function updateSymbolFromInstance(document: WebDesignDocument, componentId: string): WebDesignDocument {
  const selected = document.components.find((component) => component.id === componentId);
  if (!selected?.symbolId || !selected.symbolInstanceId) throw new Error('Selected component is not a reusable component instance.');
  const symbol = document.symbols?.find((candidate) => candidate.id === selected.symbolId);
  if (!symbol) throw new Error(`Reusable component not found: ${selected.symbolId}`);
  const instanceComponents = document.components.filter((component) => component.symbolId === symbol.id && component.symbolInstanceId === selected.symbolInstanceId);
  const byDefinitionId = new Map(instanceComponents.flatMap((component) => component.symbolComponentId ? [[component.symbolComponentId, component] as const] : []));
  const definitionOrigin = Object.fromEntries((['desktop', 'tablet', 'mobile'] as const).map((device) => [device, minimumFrame(symbol.components, device)])) as Record<WebDesignDevice, { x: number; y: number }>;
  const instanceOrigin = Object.fromEntries((['desktop', 'tablet', 'mobile'] as const).map((device) => [device, minimumFrame(instanceComponents, device)])) as Record<WebDesignDevice, { x: number; y: number }>;
  const components = symbol.components.map((definition) => {
    const instance = byDefinitionId.get(definition.id);
    if (!instance) return definition;
    let next = structuredClone(definition);
    next.name = instance.name;
    next.content = instance.content;
    next.interaction = structuredClone(instance.interaction);
    next.style = structuredClone(instance.style);
    next.layout = structuredClone(instance.layout);
    for (const device of ['desktop', 'tablet', 'mobile'] as const) {
      const frame = resolveComponent(instance, device);
      next = updateComponentFrame(next, device, {
        x: frame.x - instanceOrigin[device].x + definitionOrigin[device].x,
        y: frame.y - instanceOrigin[device].y + definitionOrigin[device].y,
        width: frame.width,
        height: frame.height,
        hidden: frame.hidden
      });
      if (device !== 'desktop') {
        const instanceStyle = instance.responsive?.[device]?.style;
        const override = next.responsive?.[device];
        if (override) next.responsive = { ...next.responsive, [device]: { ...override, style: structuredClone(instanceStyle) } };
      }
    }
    return next;
  });
  const next = {
    ...document,
    symbols: (document.symbols ?? []).map((candidate) => candidate.id === symbol.id ? { ...candidate, components } : candidate)
  };
  return syncSymbolInstances(next, symbol.id);
}

export function detachSymbolInstance(document: WebDesignDocument, componentId: string): WebDesignDocument {
  const selected = document.components.find((component) => component.id === componentId);
  if (!selected?.symbolInstanceId) return document;
  return {
    ...document,
    components: document.components.map((component) => component.symbolInstanceId === selected.symbolInstanceId ? {
      ...component,
      symbolId: undefined,
      symbolInstanceId: undefined,
      symbolComponentId: undefined,
      symbolOverrides: undefined
    } : component)
  };
}

export function setSymbolOverride(component: WebDesignComponent, override: WebSymbolOverride, enabled: boolean): WebDesignComponent {
  if (!component.symbolInstanceId) return component;
  const overrides = new Set(component.symbolOverrides ?? []);
  if (enabled) overrides.add(override);
  else overrides.delete(override);
  return { ...component, symbolOverrides: [...overrides] };
}

export function breakpointFor(document: WebDesignDocument, device: WebDesignDevice): WebDesignBreakpoint {
  if (device === 'desktop') {
    return document.breakpoints?.desktop ?? { width: document.viewport.width, height: document.viewport.height };
  }
  return document.breakpoints?.[device] ?? DEFAULT_WEB_DESIGN_BREAKPOINTS[device];
}

export function resolveComponent(component: WebDesignComponent, device: WebDesignDevice): ResolvedWebDesignComponent {
  if (device === 'desktop') {
    return {
      x: component.x,
      y: component.y,
      width: component.width,
      height: component.height,
      hidden: component.hidden === true,
      style: component.style
    };
  }
  const override = component.responsive?.[device];
  return {
    x: override?.x ?? component.x,
    y: override?.y ?? component.y,
    width: override?.width ?? component.width,
    height: override?.height ?? component.height,
    hidden: override?.hidden ?? component.hidden === true,
    style: { ...component.style, ...override?.style }
  };
}

export function updateComponentFrame(
  component: WebDesignComponent,
  device: WebDesignDevice,
  changes: Partial<Pick<WebComponentResponsiveOverride, 'x' | 'y' | 'width' | 'height' | 'hidden'>>
): WebDesignComponent {
  if (device === 'desktop') return { ...component, ...changes };
  const resolved = resolveComponent(component, device);
  const override: WebComponentResponsiveOverride = {
    x: changes.x ?? resolved.x,
    y: changes.y ?? resolved.y,
    width: changes.width ?? resolved.width,
    height: changes.height ?? resolved.height,
    hidden: changes.hidden ?? resolved.hidden,
    style: component.responsive?.[device]?.style
  };
  return { ...component, responsive: { ...component.responsive, [device]: override } };
}

export function updateComponentStyle(
  component: WebDesignComponent,
  device: WebDesignDevice,
  changes: Partial<WebComponentStyle>
): WebDesignComponent {
  if (device === 'desktop') return { ...component, style: { ...component.style, ...changes } };
  const resolved = resolveComponent(component, device);
  const override = component.responsive?.[device] ?? {
    x: resolved.x,
    y: resolved.y,
    width: resolved.width,
    height: resolved.height
  };
  return {
    ...component,
    responsive: {
      ...component.responsive,
      [device]: { ...override, style: { ...override.style, ...changes } }
    }
  };
}

export function scaleFrameForBreakpoint(
  component: WebDesignComponent,
  source: WebDesignBreakpoint,
  target: WebDesignBreakpoint
): WebComponentResponsiveOverride {
  const horizontalScale = target.width / source.width;
  const verticalScale = Math.min(1, target.height / source.height);
  return {
    x: Math.round(component.x * horizontalScale),
    y: Math.round(component.y * verticalScale),
    width: Math.max(24, Math.round(component.width * horizontalScale)),
    height: Math.max(24, Math.round(component.height * verticalScale))
  };
}

export function componentsForPage(document: WebDesignDocument, pageId: string): WebDesignComponent[] {
  return document.components.filter((component) => pageIdForComponent(document, component) === pageId);
}

export function childrenOf(document: WebDesignDocument, parentId?: string, pageId?: string): WebDesignComponent[] {
  return document.components
    .filter((component) => component.parentId === parentId && (!pageId || pageIdForComponent(document, component) === pageId))
    .sort((left, right) => left.zIndex - right.zIndex);
}

export function descendantIds(document: WebDesignDocument, componentId: string): string[] {
  const result: string[] = [];
  const visit = (parentId: string) => {
    for (const child of childrenOf(document, parentId)) {
      result.push(child.id);
      visit(child.id);
    }
  };
  visit(componentId);
  return result;
}

export function flattenComponentTree(document: WebDesignDocument, pageId?: string): FlattenedWebDesignComponent[] {
  const result: FlattenedWebDesignComponent[] = [];
  const visit = (parentId: string | undefined, depth: number) => {
    for (const component of childrenOf(document, parentId, pageId)) {
      result.push({ component, depth });
      visit(component.id, depth + 1);
    }
  };
  visit(undefined, 0);
  return result;
}

export function cloneComponentSubtrees(
  document: WebDesignDocument,
  componentIds: string[],
  targetPageId: string,
  offset = 20,
  targetDocument: WebDesignDocument = document
): ClonedComponentSubtrees {
  const roots = selectedRootIds(document, componentIds);
  const copiedIds = new Set(roots.flatMap((id) => [id, ...descendantIds(document, id)]));
  const source = document.components.filter((component) => copiedIds.has(component.id));
  const idMap = new Map(source.map((component) => [component.id, `${component.type}-${globalThis.crypto.randomUUID().slice(0, 8)}`]));
  const instanceIdMap = new Map(source.flatMap((component) => component.symbolInstanceId
    ? [[component.symbolInstanceId, `instance-${globalThis.crypto.randomUUID().slice(0, 8)}`] as const]
    : []));
  const maxZ = Math.max(0, ...componentsForPage(targetDocument, targetPageId).map((component) => component.zIndex));
  const zRanks = new Map([...source].sort((left, right) => left.zIndex - right.zIndex).map((component, index) => [component.id, index + 1]));
  const components = source.map((component) => {
    const clone = structuredClone(component);
    clone.id = idMap.get(component.id)!;
    clone.name = `${component.name} 副本`;
    clone.pageId = targetPageId;
    clone.parentId = component.parentId && idMap.has(component.parentId)
      ? idMap.get(component.parentId)
      : pageIdForComponent(document, component) === targetPageId
        && targetDocument.components.some((candidate) => candidate.id === component.parentId) ? component.parentId : undefined;
    if (!clone.parentId) clone.slot = undefined;
    if (component.symbolInstanceId) clone.symbolInstanceId = instanceIdMap.get(component.symbolInstanceId);
    clone.x += offset;
    clone.y += offset;
    clone.zIndex = maxZ + zRanks.get(component.id)!;
    clone.annotations = [];
    if (clone.responsive) {
      for (const device of ['tablet', 'mobile'] as const) {
        const frame = clone.responsive[device];
        if (frame) clone.responsive[device] = { ...frame, x: frame.x + offset, y: frame.y + offset };
      }
    }
    return clone;
  });
  return { components, rootIds: roots.map((id) => idMap.get(id)!) };
}

export function selectedRootIds(document: WebDesignDocument, componentIds: string[]): string[] {
  const selected = new Set(componentIds);
  const byId = new Map(document.components.map((component) => [component.id, component]));
  return componentIds.filter((componentId) => {
    let parentId = byId.get(componentId)?.parentId;
    while (parentId) {
      if (selected.has(parentId)) return false;
      parentId = byId.get(parentId)?.parentId;
    }
    return true;
  });
}

export function moveComponentsWithDescendants(
  document: WebDesignDocument,
  componentIds: string[],
  device: WebDesignDevice,
  dx: number,
  dy: number
): WebDesignDocument {
  const movingIds = new Set<string>();
  for (const rootId of selectedRootIds(document, componentIds)) {
    movingIds.add(rootId);
    descendantIds(document, rootId).forEach((id) => movingIds.add(id));
  }
  return {
    ...document,
    components: document.components.map((component) => {
      if (!movingIds.has(component.id)) return component;
      const frame = resolveComponent(component, device);
      return updateComponentFrame(component, device, { x: Math.round(frame.x + dx), y: Math.round(frame.y + dy) });
    })
  };
}

function alignedOffset(available: number, size: number, align: 'start' | 'center' | 'end' | 'stretch'): number {
  if (align === 'center') return (available - size) / 2;
  if (align === 'end') return available - size;
  return 0;
}

export function autoLayoutContainer(
  document: WebDesignDocument,
  containerId: string,
  device: WebDesignDevice
): WebDesignDocument {
  const container = document.components.find((component) => component.id === containerId);
  if (!container) throw new Error(`Component not found: ${containerId}`);
  const layout = container.layout ?? { mode: 'free', gap: 16, padding: 16, align: 'start' as const };
  if (layout.mode === 'free') return document;
  const children = childrenOf(document, containerId);
  if (children.length === 0) return document;
  const parentFrame = resolveComponent(container, device);
  const padding = layout.padding;
  const gap = layout.gap;
  const align = layout.align ?? 'start';
  const innerWidth = Math.max(16, parentFrame.width - padding * 2);
  const innerHeight = Math.max(16, parentFrame.height - padding * 2);
  const frames = new Map<string, Partial<Pick<ResolvedWebDesignComponent, 'x' | 'y' | 'width' | 'height'>>>();

  if (layout.mode === 'flex-row') {
    let cursor = parentFrame.x + padding;
    for (const child of children) {
      const frame = resolveComponent(child, device);
      const height = align === 'stretch' ? innerHeight : Math.min(frame.height, innerHeight);
      frames.set(child.id, {
        x: cursor,
        y: parentFrame.y + padding + alignedOffset(innerHeight, height, align),
        width: frame.width,
        height
      });
      cursor += frame.width + gap;
    }
  } else if (layout.mode === 'flex-column') {
    let cursor = parentFrame.y + padding;
    for (const child of children) {
      const frame = resolveComponent(child, device);
      const width = align === 'stretch' ? innerWidth : Math.min(frame.width, innerWidth);
      frames.set(child.id, {
        x: parentFrame.x + padding + alignedOffset(innerWidth, width, align),
        y: cursor,
        width,
        height: frame.height
      });
      cursor += frame.height + gap;
    }
  } else {
    const columns = Math.max(1, Math.min(Math.floor(layout.columns ?? 2), children.length));
    const cellWidth = Math.max(16, (innerWidth - gap * (columns - 1)) / columns);
    const rows: WebDesignComponent[][] = [];
    for (let index = 0; index < children.length; index += columns) rows.push(children.slice(index, index + columns));
    let cursorY = parentFrame.y + padding;
    for (const row of rows) {
      const rowHeight = Math.max(...row.map((child) => resolveComponent(child, device).height));
      row.forEach((child, column) => {
        const frame = resolveComponent(child, device);
        const width = align === 'stretch' ? cellWidth : Math.min(frame.width, cellWidth);
        frames.set(child.id, {
          x: parentFrame.x + padding + column * (cellWidth + gap) + alignedOffset(cellWidth, width, align),
          y: cursorY,
          width,
          height: frame.height
        });
      });
      cursorY += rowHeight + gap;
    }
  }

  const descendantMovement = new Map<string, { dx: number; dy: number }>();
  for (const child of children) {
    const changes = frames.get(child.id);
    if (!changes || changes.x === undefined || changes.y === undefined) continue;
    const frame = resolveComponent(child, device);
    const movement = { dx: changes.x - frame.x, dy: changes.y - frame.y };
    descendantIds(document, child.id).forEach((id) => descendantMovement.set(id, movement));
  }

  return {
    ...document,
    components: document.components.map((component) => {
      const changes = frames.get(component.id);
      if (changes) return updateComponentFrame(component, device, changes);
      const movement = descendantMovement.get(component.id);
      if (!movement) return component;
      const frame = resolveComponent(component, device);
      return updateComponentFrame(component, device, { x: frame.x + movement.dx, y: frame.y + movement.dy });
    })
  };
}

type AxisCandidate = { delta: number; guide: number };

function bestCandidate(candidates: AxisCandidate[], threshold: number): AxisCandidate | undefined {
  return candidates
    .filter((candidate) => Math.abs(candidate.delta) <= threshold)
    .sort((left, right) => Math.abs(left.delta) - Math.abs(right.delta))[0];
}

export function snapComponentFrame(
  document: WebDesignDocument,
  componentId: string,
  device: WebDesignDevice,
  frame: ResolvedWebDesignComponent,
  excludedIds: string[] = [componentId],
  threshold = 7
): SnapResult {
  const breakpoint = breakpointFor(document, device);
  const excluded = new Set(excludedIds);
  const movingComponent = document.components.find((component) => component.id === componentId);
  const pageId = movingComponent ? pageIdForComponent(document, movingComponent) : undefined;
  const targetX = [0, breakpoint.width / 2, breakpoint.width];
  const targetY = [0, breakpoint.height / 2, breakpoint.height];
  for (const component of document.components) {
    if (excluded.has(component.id)) continue;
    if (pageId && pageIdForComponent(document, component) !== pageId) continue;
    const other = resolveComponent(component, device);
    if (other.hidden) continue;
    targetX.push(other.x, other.x + other.width / 2, other.x + other.width);
    targetY.push(other.y, other.y + other.height / 2, other.y + other.height);
  }
  const sourceX = [frame.x, frame.x + frame.width / 2, frame.x + frame.width];
  const sourceY = [frame.y, frame.y + frame.height / 2, frame.y + frame.height];
  const x = bestCandidate(targetX.flatMap((target) => sourceX.map((source) => ({ delta: target - source, guide: target }))), threshold);
  const y = bestCandidate(targetY.flatMap((target) => sourceY.map((source) => ({ delta: target - source, guide: target }))), threshold);
  return {
    frame: { ...frame, x: Math.round(frame.x + (x?.delta ?? 0)), y: Math.round(frame.y + (y?.delta ?? 0)) },
    guides: { x: x?.guide, y: y?.guide }
  };
}
