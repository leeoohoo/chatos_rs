import {
  DEFAULT_WEB_DESIGN_BREAKPOINTS,
  pageIdForComponent,
  type WebComponentResponsiveOverride,
  type WebComponentStyle,
  type WebDesignBreakpoint,
  type WebDesignComponent,
  type WebDesignDevice,
  type WebDesignDocument,
  type WebHorizontalConstraint,
  type WebDesignSymbol,
  type WebSymbolOverride
} from './schema.js';
import {
  componentsForPage,
  descendantIds,
  resolveComponent,
  selectedRootIds,
  updateComponentFrame,
  type ClonedComponentSubtrees
} from './editor-model.js';

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
    next.states = structuredClone(definition.states);
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
    next.states = structuredClone(instance.states);
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
