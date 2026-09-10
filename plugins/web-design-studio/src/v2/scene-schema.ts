import { assertSceneGridTracks } from './grid-tracks.js';

export type SceneNodeId = string;
export type SceneNodeType =
  | 'section'
  | 'frame'
  | 'group'
  | 'text'
  | 'shape'
  | 'media'
  | 'library-instance'
  | 'component-main'
  | 'component-set'
  | 'component-instance';
export type SceneCreator = 'human' | 'ai' | 'system' | `integration:${string}`;
export type SceneSizingMode = 'fixed' | 'hug' | 'fill';
export type ScenePositionMode = 'flow' | 'absolute';
export type SceneLayoutMode = 'free' | 'auto' | 'grid';
export type SceneVariableType = 'color' | 'number' | 'string' | 'boolean' | 'duration' | 'easing';
export type SceneVariableValue = string | number | boolean;

export interface SceneRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SceneTransform {
  rotation: number;
  scaleX: number;
  scaleY: number;
  skewX: number;
  skewY: number;
}

export interface SceneLayout {
  mode: SceneLayoutMode;
  direction?: 'horizontal' | 'vertical';
  wrap?: boolean;
  padding: { top: number; right: number; bottom: number; left: number };
  gap: { row: number; column: number };
  alignItems?: 'start' | 'center' | 'end' | 'stretch' | 'baseline';
  justifyContent?: 'start' | 'center' | 'end' | 'between' | 'around' | 'evenly';
  sizingX: SceneSizingMode;
  sizingY: SceneSizingMode;
  minWidth?: number;
  maxWidth?: number;
  minHeight?: number;
  maxHeight?: number;
  position: ScenePositionMode;
  clipContent: boolean;
  grid?: {
    columns: string[];
    rows: string[];
    autoFlow: 'row' | 'column' | 'dense';
  };
  gridPlacement?: {
    columnStart?: number;
    rowStart?: number;
    columnSpan?: number;
    rowSpan?: number;
  };
  constraints?: {
    horizontal: 'left' | 'center' | 'right' | 'stretch' | 'scale';
    vertical: 'top' | 'center' | 'bottom' | 'stretch' | 'scale';
  };
}

export interface ScenePaint {
  type: 'solid' | 'linear-gradient' | 'radial-gradient' | 'image';
  visible: boolean;
  opacity: number;
  color?: string;
  stops?: Array<{ offset: number; color: string }>;
  imageAssetId?: string;
  imageFit?: 'fill' | 'fit' | 'crop' | 'tile';
}

export interface SceneStroke {
  paint: ScenePaint;
  width: { top: number; right: number; bottom: number; left: number };
  style: 'solid' | 'dashed' | 'dotted';
}

export interface SceneEffect {
  type: 'drop-shadow' | 'inner-shadow' | 'layer-blur' | 'background-blur';
  visible: boolean;
  radius: number;
  color?: string;
  offset?: { x: number; y: number };
  spread?: number;
}

export interface SceneTypography {
  fontFamily: string;
  fontSize: number;
  fontWeight: number;
  lineHeight: number;
  letterSpacing: number;
  textAlign: 'left' | 'center' | 'right' | 'justify';
}

export interface SceneAppearance {
  opacity: number;
  blendMode: 'normal' | 'multiply' | 'screen' | 'overlay' | 'darken' | 'lighten';
  fills: ScenePaint[];
  strokes: SceneStroke[];
  effects: SceneEffect[];
  radius: { topLeft: number; topRight: number; bottomRight: number; bottomLeft: number };
  typography?: SceneTypography;
}

export interface SceneAnnotation {
  id: string;
  author: SceneCreator;
  body: string;
  status: 'open' | 'resolved';
  createdAt: string;
  resolvedAt?: string;
}

export interface SceneAiPolicy {
  editable: boolean;
  lockedFields: string[];
  intent?: string;
}

export interface ScenePrototypeLink {
  trigger: 'click';
  action: 'navigate' | 'overlay';
  targetPageId: string;
}

export interface SceneNodeBase {
  id: SceneNodeId;
  type: SceneNodeType;
  name: string;
  role?: string;
  visible: boolean;
  locked: boolean;
  frame: SceneRect;
  transform: SceneTransform;
  layout: SceneLayout;
  appearance: SceneAppearance;
  variableBindings: Record<string, string>;
  annotations: SceneAnnotation[];
  aiPolicy: SceneAiPolicy;
  prototypeLink?: ScenePrototypeLink;
  createdBy: SceneCreator;
  updatedBy: SceneCreator;
  createdAt: string;
  updatedAt: string;
}

export interface SceneSectionNode extends SceneNodeBase {
  type: 'section';
  children: SceneNode[];
}

export interface SceneFrameNode extends SceneNodeBase {
  type: 'frame';
  children: SceneNode[];
}

export interface SceneGroupNode extends SceneNodeBase {
  type: 'group';
  children: SceneNode[];
}

export interface SceneTextNode extends SceneNodeBase {
  type: 'text';
  content: string;
}

export interface SceneShapeNode extends SceneNodeBase {
  type: 'shape';
  shape: 'rectangle' | 'ellipse' | 'line' | 'polygon' | 'star' | 'vector';
  pathData?: string;
}

export interface SceneMediaNode extends SceneNodeBase {
  type: 'media';
  mediaType: 'image' | 'video' | 'audio';
  assetId: string;
  alt?: string;
  intrinsicSize?: { width: number; height: number };
  preserveAspectRatio: boolean;
}

export interface SceneLibraryInstanceNode extends SceneNodeBase {
  type: 'library-instance';
  library: string;
  component: string;
  variant?: string;
  content?: string;
  properties: Record<string, unknown>;
  slots: Record<string, SceneNode[]>;
}

export interface SceneComponentMainNode extends SceneNodeBase {
  type: 'component-main';
  propertyDefinitions: Record<string, SceneComponentPropertyDefinition>;
  children: SceneNode[];
}

export type SceneComponentPropertyDefinition =
  | { type: 'boolean'; defaultValue: boolean }
  | { type: 'text'; defaultValue: string }
  | { type: 'instance-swap'; defaultValue?: string }
  | { type: 'variant'; defaultValue: string; options: string[] }
  | { type: 'slot'; acceptedNodeTypes?: SceneNodeType[]; minItems?: number; maxItems?: number };

export interface SceneComponentSetNode extends SceneNodeBase {
  type: 'component-set';
  variantProperties: string[];
  children: SceneComponentMainNode[];
}

export interface SceneComponentInstanceNode extends SceneNodeBase {
  type: 'component-instance';
  mainComponentId: string;
  overrides: Record<string, unknown>;
  slots: Record<string, SceneNode[]>;
}

export type SceneContainerNode = SceneSectionNode | SceneFrameNode | SceneGroupNode | SceneComponentMainNode | SceneComponentSetNode;
export type SceneNode =
  | SceneContainerNode
  | SceneTextNode
  | SceneShapeNode
  | SceneMediaNode
  | SceneLibraryInstanceNode
  | SceneComponentInstanceNode;
export type SceneSlotContainerNode = SceneLibraryInstanceNode | SceneComponentInstanceNode;

export interface ScenePage {
  id: string;
  name: string;
  children: Array<SceneSectionNode | SceneFrameNode | SceneComponentMainNode | SceneComponentSetNode>;
}

export interface SceneVariable {
  id: string;
  name: string;
  type: SceneVariableType;
  valuesByMode: Record<string, SceneVariableValue>;
  aliasByMode?: Record<string, string>;
}

export interface SceneVariableCollection {
  id: string;
  name: string;
  modes: Array<{ id: string; name: string }>;
  variables: SceneVariable[];
}

export interface SceneDocument {
  schemaVersion: 2;
  documentId: string;
  revision: number;
  name: string;
  pages: ScenePage[];
  variableCollections: SceneVariableCollection[];
  responsiveRules: SceneResponsiveRule[];
  createdAt: string;
  updatedAt: string;
}

export interface SceneResponsiveNodeOverride {
  nodeId: string;
  visible?: boolean;
  layout?: Partial<SceneLayout>;
  childOrder?: string[];
}

export interface SceneResponsiveRule {
  id: string;
  name: string;
  minWidth?: number;
  maxWidth?: number;
  variableModes: Record<string, string>;
  nodeOverrides: SceneResponsiveNodeOverride[];
}

export interface SceneIndexEntry {
  node: SceneNode;
  parentId: string;
  pageId: string;
  path: number[];
}

const identifierPattern = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/;
const dangerousPathSegments = new Set(['__proto__', 'prototype', 'constructor']);
const variableTypes = new Set<SceneVariableType>(['color', 'number', 'string', 'boolean', 'duration', 'easing']);
const sceneNodeTypeSet = new Set<SceneNodeType>(['section', 'frame', 'group', 'text', 'shape', 'media', 'library-instance', 'component-main', 'component-set', 'component-instance']);
const protectedOverrideRoots = new Set(['id', 'type', 'children', 'slots', 'createdAt', 'createdBy', 'updatedAt', 'updatedBy']);

function assertIdentifier(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !identifierPattern.test(value)) throw new Error(`${label} is invalid.`);
}

function assertFinite(value: unknown, label: string, minimum?: number): asserts value is number {
  if (typeof value !== 'number' || !Number.isFinite(value) || (minimum !== undefined && value < minimum)) {
    throw new Error(`${label} is invalid.`);
  }
}

function assertTimestamp(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !Number.isFinite(Date.parse(value))) throw new Error(`${label} is invalid.`);
}

function assertRect(rect: SceneRect, label: string): void {
  assertFinite(rect.x, `${label}.x`);
  assertFinite(rect.y, `${label}.y`);
  assertFinite(rect.width, `${label}.width`, 0);
  assertFinite(rect.height, `${label}.height`, 0);
}

function assertLayout(layout: SceneLayout, label: string): void {
  if (!['free', 'auto', 'grid'].includes(layout.mode)) throw new Error(`${label}.mode is invalid.`);
  if (!['fixed', 'hug', 'fill'].includes(layout.sizingX) || !['fixed', 'hug', 'fill'].includes(layout.sizingY)) {
    throw new Error(`${label} sizing is invalid.`);
  }
  if (!['flow', 'absolute'].includes(layout.position)) throw new Error(`${label}.position is invalid.`);
  for (const [side, value] of Object.entries(layout.padding)) assertFinite(value, `${label}.padding.${side}`, 0);
  for (const [axis, value] of Object.entries(layout.gap)) assertFinite(value, `${label}.gap.${axis}`, 0);
  for (const [name, value] of Object.entries({ minWidth: layout.minWidth, maxWidth: layout.maxWidth, minHeight: layout.minHeight, maxHeight: layout.maxHeight })) {
    if (value !== undefined) assertFinite(value, `${label}.${name}`, 0);
  }
  if (layout.minWidth !== undefined && layout.maxWidth !== undefined && layout.minWidth > layout.maxWidth) throw new Error(`${label} width bounds are invalid.`);
  if (layout.minHeight !== undefined && layout.maxHeight !== undefined && layout.minHeight > layout.maxHeight) throw new Error(`${label} height bounds are invalid.`);
  if (layout.mode === 'auto' && !layout.direction) throw new Error(`${label}.direction is required for auto layout.`);
  if (layout.mode === 'grid') {
    if (!layout.grid) throw new Error(`${label}.grid is required for grid layout.`);
    if (!['row', 'column', 'dense'].includes(layout.grid.autoFlow)) throw new Error(`${label}.grid.autoFlow is invalid.`);
    assertSceneGridTracks(layout.grid.columns, `${label}.grid.columns`);
    assertSceneGridTracks(layout.grid.rows, `${label}.grid.rows`, true);
  }
  if (layout.gridPlacement) {
    for (const [property, value] of Object.entries(layout.gridPlacement)) {
      if (!Number.isSafeInteger(value) || value < 1) throw new Error(`${label}.gridPlacement.${property} is invalid.`);
    }
  }
  if (layout.constraints) {
    if (!['left', 'center', 'right', 'stretch', 'scale'].includes(layout.constraints.horizontal)) throw new Error(`${label}.constraints.horizontal is invalid.`);
    if (!['top', 'center', 'bottom', 'stretch', 'scale'].includes(layout.constraints.vertical)) throw new Error(`${label}.constraints.vertical is invalid.`);
  }
}

export function mergeSceneLayout(base: SceneLayout, override: Partial<SceneLayout>): SceneLayout {
  return {
    ...base,
    ...override,
    padding: override.padding ? { ...base.padding, ...override.padding } : { ...base.padding },
    gap: override.gap ? { ...base.gap, ...override.gap } : { ...base.gap },
    ...(override.grid === undefined ? { ...(base.grid === undefined ? {} : { grid: structuredClone(base.grid) }) } : { grid: structuredClone(override.grid) }),
    ...(override.gridPlacement === undefined ? { ...(base.gridPlacement === undefined ? {} : { gridPlacement: { ...base.gridPlacement } }) } : { gridPlacement: { ...base.gridPlacement, ...override.gridPlacement } }),
    ...(override.constraints === undefined ? { ...(base.constraints === undefined ? {} : { constraints: { ...base.constraints } }) } : { constraints: { ...base.constraints, ...override.constraints } })
  };
}

function assertAppearance(appearance: SceneAppearance, label: string): void {
  assertFinite(appearance.opacity, `${label}.opacity`, 0);
  if (appearance.opacity > 1) throw new Error(`${label}.opacity is invalid.`);
  for (const [corner, value] of Object.entries(appearance.radius)) assertFinite(value, `${label}.radius.${corner}`, 0);
  for (const [index, paint] of appearance.fills.entries()) {
    assertFinite(paint.opacity, `${label}.fills[${index}].opacity`, 0);
    if (paint.opacity > 1) throw new Error(`${label}.fills[${index}].opacity is invalid.`);
  }
  if (appearance.typography) {
    assertFinite(appearance.typography.fontSize, `${label}.typography.fontSize`, 0);
    assertFinite(appearance.typography.fontWeight, `${label}.typography.fontWeight`, 1);
    assertFinite(appearance.typography.lineHeight, `${label}.typography.lineHeight`, 0);
    assertFinite(appearance.typography.letterSpacing, `${label}.typography.letterSpacing`);
  }
}

function readPath(target: unknown, path: string[]): { exists: boolean; value: unknown } {
  let cursor = target;
  for (const segment of path) {
    if (cursor === null || typeof cursor !== 'object' || !Object.hasOwn(cursor, segment)) return { exists: false, value: undefined };
    cursor = (cursor as Record<string, unknown>)[segment];
  }
  return { exists: true, value: cursor };
}

export function bindableVariableTypesForScenePath(node: SceneNode, propertyPath: string): SceneVariableType[] {
  const path = propertyPath.split('.');
  if (path.some((segment) => !segment || dangerousPathSegments.has(segment))) return [];
  const joined = path.join('.');
  const numberPatterns = [
    /^frame\.(x|y|width|height)$/,
    /^transform\.(rotation|scaleX|scaleY|skewX|skewY)$/,
    /^layout\.(minWidth|maxWidth|minHeight|maxHeight)$/,
    /^layout\.padding\.(top|right|bottom|left)$/,
    /^layout\.gap\.(row|column)$/,
    /^appearance\.opacity$/,
    /^appearance\.radius\.(topLeft|topRight|bottomRight|bottomLeft)$/,
    /^appearance\.fills\.\d+\.opacity$/,
    /^appearance\.fills\.\d+\.stops\.\d+\.offset$/,
    /^appearance\.strokes\.\d+\.width\.(top|right|bottom|left)$/,
    /^appearance\.strokes\.\d+\.paint\.opacity$/,
    /^appearance\.strokes\.\d+\.paint\.stops\.\d+\.offset$/,
    /^appearance\.effects\.\d+\.(radius|spread)$/,
    /^appearance\.effects\.\d+\.offset\.(x|y)$/,
    /^appearance\.typography\.(fontSize|fontWeight|lineHeight|letterSpacing)$/
  ];
  const colorPatterns = [
    /^appearance\.fills\.\d+\.color$/,
    /^appearance\.fills\.\d+\.stops\.\d+\.color$/,
    /^appearance\.strokes\.\d+\.paint\.color$/,
    /^appearance\.strokes\.\d+\.paint\.stops\.\d+\.color$/,
    /^appearance\.effects\.\d+\.color$/
  ];
  if (numberPatterns.some((pattern) => pattern.test(joined))) return ['number'];
  if (colorPatterns.some((pattern) => pattern.test(joined))) return ['color'];
  if (/^(visible|layout\.wrap|layout\.clipContent)$/.test(joined)) return ['boolean'];
  if (/^(content|alt|appearance\.typography\.fontFamily|appearance\.typography\.textAlign)$/.test(joined)) return ['string'];
  if (/^(properties|overrides)\./.test(joined)) {
    const current = readPath(node, path);
    if (!current.exists) return [];
    if (typeof current.value === 'boolean') return ['boolean'];
    if (typeof current.value === 'number') return ['number', 'duration'];
    if (typeof current.value === 'string') return ['string', 'color', 'easing'];
  }
  return [];
}

function assertVariableValue(value: SceneVariableValue, type: SceneVariableType, label: string): void {
  if ((type === 'number' || type === 'duration') && (typeof value !== 'number' || !Number.isFinite(value))) throw new Error(`${label} must be a finite number.`);
  if (type === 'boolean' && typeof value !== 'boolean') throw new Error(`${label} must be a boolean.`);
  if ((type === 'color' || type === 'string' || type === 'easing') && typeof value !== 'string') throw new Error(`${label} must be a string.`);
}

function assertVariableBindings(node: SceneNode, variables: Map<string, SceneVariable>): void {
  if (!node.variableBindings || typeof node.variableBindings !== 'object' || Array.isArray(node.variableBindings)) {
    throw new Error(`Node ${node.id} variableBindings are invalid.`);
  }
  for (const [propertyPath, variableId] of Object.entries(node.variableBindings)) {
    if (!propertyPath || typeof variableId !== 'string') throw new Error(`Node ${node.id} has an invalid variable binding.`);
    const variable = variables.get(variableId);
    if (!variable) throw new Error(`Node ${node.id} binds unknown variable ${variableId}.`);
    const allowedTypes = bindableVariableTypesForScenePath(node, propertyPath);
    if (allowedTypes.length === 0) throw new Error(`Node ${node.id} variable binding path ${propertyPath} is not bindable.`);
    if (!allowedTypes.includes(variable.type)) {
      throw new Error(`Node ${node.id} cannot bind ${variable.type} variable ${variableId} to ${propertyPath}.`);
    }
  }
}

function parseOverridePointer(pointer: string): string[] {
  if (!pointer.startsWith('/')) throw new Error(`Component override ${pointer} must be a JSON Pointer.`);
  const segments = pointer.slice(1).split('/').map((segment) => {
    if (/~(?![01])/.test(segment)) throw new Error(`Component override ${pointer} has invalid escaping.`);
    return segment.replaceAll('~1', '/').replaceAll('~0', '~');
  });
  if (segments.some((segment) => !segment || dangerousPathSegments.has(segment))) throw new Error(`Component override ${pointer} is invalid.`);
  return segments;
}

function assertComponentPropertyValue(
  value: unknown,
  definition: Exclude<SceneComponentPropertyDefinition, { type: 'slot' }>,
  propertyName: string,
  componentMains: Map<string, SceneComponentMainNode>
): void {
  if (definition.type === 'boolean' && typeof value !== 'boolean') throw new Error(`Component property ${propertyName} needs a boolean value.`);
  if (definition.type === 'text' && typeof value !== 'string') throw new Error(`Component property ${propertyName} needs a text value.`);
  if (definition.type === 'variant' && (typeof value !== 'string' || !definition.options.includes(value))) {
    throw new Error(`Component property ${propertyName} needs a valid variant value.`);
  }
  if (definition.type === 'instance-swap' && value !== undefined && (typeof value !== 'string' || !componentMains.has(value))) {
    throw new Error(`Component property ${propertyName} needs a valid main component id.`);
  }
}

function assertComponentDefinitions(main: SceneComponentMainNode, componentMains: Map<string, SceneComponentMainNode>): void {
  if (!main.propertyDefinitions || typeof main.propertyDefinitions !== 'object' || Array.isArray(main.propertyDefinitions)) {
    throw new Error(`Main component ${main.id} property definitions are invalid.`);
  }
  for (const [propertyName, definition] of Object.entries(main.propertyDefinitions)) {
    if (!propertyName.trim() || propertyName.includes('/')) throw new Error(`Main component ${main.id} has an invalid property name.`);
    if (!definition || typeof definition !== 'object') throw new Error(`Component property ${propertyName} is invalid.`);
    if (definition.type === 'slot') {
      if (definition.acceptedNodeTypes !== undefined) {
        if (!Array.isArray(definition.acceptedNodeTypes) || definition.acceptedNodeTypes.length === 0
          || definition.acceptedNodeTypes.some((type) => !sceneNodeTypeSet.has(type) || type === 'section')) {
          throw new Error(`Component slot ${propertyName} accepted node types are invalid.`);
        }
      }
      for (const [label, count] of [['minItems', definition.minItems], ['maxItems', definition.maxItems]] as const) {
        if (count !== undefined && (!Number.isSafeInteger(count) || count < 0)) throw new Error(`Component slot ${propertyName} ${label} is invalid.`);
      }
      if (definition.minItems !== undefined && definition.maxItems !== undefined && definition.minItems > definition.maxItems) {
        throw new Error(`Component slot ${propertyName} item bounds are invalid.`);
      }
      continue;
    }
    if (definition.type === 'variant') {
      if (!Array.isArray(definition.options) || definition.options.length === 0 || new Set(definition.options).size !== definition.options.length
        || definition.options.some((option) => typeof option !== 'string' || !option)) {
        throw new Error(`Component property ${propertyName} variant options are invalid.`);
      }
    }
    assertComponentPropertyValue(definition.defaultValue, definition, propertyName, componentMains);
  }
}

function sameRuntimeShape(left: unknown, right: unknown): boolean {
  if (left === null || right === null) return left === right;
  if (Array.isArray(left) || Array.isArray(right)) return Array.isArray(left) && Array.isArray(right);
  return typeof left === typeof right;
}

function assertComponentInstance(
  instance: SceneComponentInstanceNode,
  main: SceneComponentMainNode,
  sceneIndex: Map<string, SceneIndexEntry>,
  componentMains: Map<string, SceneComponentMainNode>
): void {
  if (!instance.overrides || typeof instance.overrides !== 'object' || Array.isArray(instance.overrides)) {
    throw new Error(`Component instance ${instance.id} overrides are invalid.`);
  }
  const mainDescendants = new Set<string>([main.id]);
  for (const entry of sceneIndex.values()) {
    let parentId = entry.parentId;
    while (parentId !== entry.pageId) {
      if (parentId === main.id) {
        mainDescendants.add(entry.node.id);
        break;
      }
      const parent = sceneIndex.get(parentId);
      if (!parent) break;
      parentId = parent.parentId;
    }
  }
  for (const [pointer, value] of Object.entries(instance.overrides)) {
    const segments = parseOverridePointer(pointer);
    if (segments[0] === 'properties' && segments.length === 2) {
      const definition = main.propertyDefinitions[segments[1]];
      if (!definition) throw new Error(`Component instance ${instance.id} overrides unknown property ${segments[1]}.`);
      if (definition.type === 'slot') throw new Error(`Component instance ${instance.id} must edit slot ${segments[1]} through slots.`);
      assertComponentPropertyValue(value, definition, segments[1], componentMains);
      continue;
    }
    if (segments[0] === 'nodes' && segments.length >= 3) {
      const targetId = segments[1];
      if (!mainDescendants.has(targetId)) throw new Error(`Component instance ${instance.id} override target ${targetId} is outside its main component.`);
      const fieldPath = segments.slice(2);
      if (protectedOverrideRoots.has(fieldPath[0])) throw new Error(`Component instance ${instance.id} cannot override ${fieldPath[0]}.`);
      const current = readPath(sceneIndex.get(targetId)!.node, fieldPath);
      if (!current.exists) throw new Error(`Component instance ${instance.id} override path ${pointer} does not exist.`);
      if (!sameRuntimeShape(current.value, value)) throw new Error(`Component instance ${instance.id} override ${pointer} has an incompatible value.`);
      continue;
    }
    throw new Error(`Component instance ${instance.id} override path ${pointer} is unsupported.`);
  }
  if (!instance.slots || typeof instance.slots !== 'object' || Array.isArray(instance.slots)) throw new Error(`Component instance ${instance.id} slots are invalid.`);
  const slotDefinitions = Object.fromEntries(Object.entries(main.propertyDefinitions).filter(([, definition]) => definition.type === 'slot')) as Record<string, Extract<SceneComponentPropertyDefinition, { type: 'slot' }>>;
  for (const slotName of Object.keys(instance.slots)) if (!slotDefinitions[slotName]) throw new Error(`Component instance ${instance.id} uses unknown slot ${slotName}.`);
  for (const [slotName, definition] of Object.entries(slotDefinitions)) {
    const children = instance.slots[slotName] ?? [];
    if (definition.minItems !== undefined && children.length < definition.minItems) throw new Error(`Component instance ${instance.id} slot ${slotName} needs at least ${definition.minItems} items.`);
    if (definition.maxItems !== undefined && children.length > definition.maxItems) throw new Error(`Component instance ${instance.id} slot ${slotName} allows at most ${definition.maxItems} items.`);
    if (definition.acceptedNodeTypes && children.some((child) => !definition.acceptedNodeTypes!.includes(child.type))) {
      throw new Error(`Component instance ${instance.id} slot ${slotName} contains an unsupported node type.`);
    }
  }
}

export function isSceneContainer(node: SceneNode): node is SceneContainerNode {
  return node.type === 'section' || node.type === 'frame' || node.type === 'group' || node.type === 'component-main' || node.type === 'component-set';
}

export function isSceneSlotContainer(node: SceneNode): node is SceneSlotContainerNode {
  return node.type === 'library-instance' || node.type === 'component-instance';
}

function visitNode(node: SceneNode, parentId: string, pageId: string, path: number[], ids: Set<string>, objects: WeakSet<object>, index?: Map<string, SceneIndexEntry>): void {
  if (objects.has(node)) throw new Error(`Scene graph contains an object cycle at ${node.id}.`);
  objects.add(node);
  assertIdentifier(node.id, 'node.id');
  if (ids.has(node.id)) throw new Error(`Duplicate scene id: ${node.id}`);
  ids.add(node.id);
  if (!node.name.trim()) throw new Error(`Scene node ${node.id} needs a name.`);
  assertRect(node.frame, `node.${node.id}.frame`);
  assertLayout(node.layout, `node.${node.id}.layout`);
  assertAppearance(node.appearance, `node.${node.id}.appearance`);
  assertTimestamp(node.createdAt, `node.${node.id}.createdAt`);
  assertTimestamp(node.updatedAt, `node.${node.id}.updatedAt`);
  if (node.prototypeLink !== undefined) {
    if (!node.prototypeLink || typeof node.prototypeLink !== 'object') throw new Error(`Node ${node.id} prototype link is invalid.`);
    if (node.prototypeLink.trigger !== 'click') throw new Error(`Node ${node.id} prototype trigger is invalid.`);
    if (!['navigate', 'overlay'].includes(node.prototypeLink.action)) throw new Error(`Node ${node.id} prototype action is invalid.`);
    assertIdentifier(node.prototypeLink.targetPageId, `node.${node.id}.prototypeLink.targetPageId`);
  }
  if ((node.type === 'group' || node.type === 'section') && node.layout.mode !== 'free') {
    throw new Error(`${node.type} ${node.id} cannot own auto or grid layout.`);
  }
  if (node.type === 'text' && typeof node.content !== 'string') throw new Error(`Text ${node.id} content is invalid.`);
  if (node.type === 'library-instance' && node.content !== undefined && typeof node.content !== 'string') {
    throw new Error(`Library instance ${node.id} content is invalid.`);
  }
  if (node.type === 'library-instance') {
    if (typeof node.library !== 'string' || !node.library.trim()) throw new Error(`Library instance ${node.id} library is invalid.`);
    if (typeof node.component !== 'string' || !node.component.trim()) throw new Error(`Library instance ${node.id} component is invalid.`);
    if (node.variant !== undefined && typeof node.variant !== 'string') throw new Error(`Library instance ${node.id} variant is invalid.`);
    if (!node.properties || typeof node.properties !== 'object' || Array.isArray(node.properties)) {
      throw new Error(`Library instance ${node.id} properties are invalid.`);
    }
  }
  if (node.type === 'media') assertIdentifier(node.assetId, `media.${node.id}.assetId`);
  if (node.type === 'media') {
    if (typeof node.preserveAspectRatio !== 'boolean') throw new Error(`Media ${node.id} preserveAspectRatio is invalid.`);
    if ((node.mediaType === 'image' || node.mediaType === 'video') && !node.intrinsicSize) throw new Error(`Media ${node.id} needs an intrinsic size.`);
    if (node.intrinsicSize) {
      assertFinite(node.intrinsicSize.width, `media.${node.id}.intrinsicSize.width`, Number.EPSILON);
      assertFinite(node.intrinsicSize.height, `media.${node.id}.intrinsicSize.height`, Number.EPSILON);
    }
  }
  index?.set(node.id, { node, parentId, pageId, path });
  if (isSceneContainer(node)) {
    for (const [childIndex, child] of node.children.entries()) {
      if (node.type === 'component-set' && child.type !== 'component-main') throw new Error(`Component set ${node.id} can contain only main components.`);
      if (child.type === 'section' && !(node.type === 'frame' && node.role === 'page-root')) {
        throw new Error(`Section ${child.id} must be a page-level node or a direct child of the page root frame.`);
      }
      visitNode(child, node.id, pageId, [...path, childIndex], ids, objects, index);
    }
  }
  if (isSceneSlotContainer(node)) {
    for (const [slotName, children] of Object.entries(node.slots)) {
      if (!slotName.trim()) throw new Error(`Scene instance ${node.id} has an unnamed slot.`);
      if (!Array.isArray(children)) throw new Error(`Scene instance ${node.id} slot ${slotName} is invalid.`);
      for (const [childIndex, child] of children.entries()) {
        if (child.type === 'section') throw new Error(`Section ${child.id} must be a page-level node.`);
        visitNode(child, node.id, pageId, [...path, childIndex], ids, objects, index);
      }
    }
  }
  objects.delete(node);
}

export function assertSceneDocument(value: unknown): asserts value is SceneDocument {
  if (!value || typeof value !== 'object') throw new Error('Scene document must be an object.');
  const document = value as SceneDocument;
  if (document.schemaVersion !== 2) throw new Error('Scene document schemaVersion must be 2.');
  assertIdentifier(document.documentId, 'documentId');
  if (!Number.isSafeInteger(document.revision) || document.revision < 0) throw new Error('Scene document revision is invalid.');
  if (typeof document.name !== 'string' || !document.name.trim()) throw new Error('Scene document name is invalid.');
  assertTimestamp(document.createdAt, 'document.createdAt');
  assertTimestamp(document.updatedAt, 'document.updatedAt');
  if (!Array.isArray(document.pages) || document.pages.length === 0) throw new Error('Scene document needs at least one page.');
  const ids = new Set<string>([document.documentId]);
  const objects = new WeakSet<object>();
  for (const [pageIndex, page] of document.pages.entries()) {
    assertIdentifier(page.id, `pages[${pageIndex}].id`);
    if (ids.has(page.id)) throw new Error(`Duplicate scene id: ${page.id}`);
    ids.add(page.id);
    if (!page.name.trim()) throw new Error(`Page ${page.id} needs a name.`);
    for (const [childIndex, child] of page.children.entries()) visitNode(child, page.id, page.id, [pageIndex, childIndex], ids, objects);
  }
  if (!Array.isArray(document.variableCollections)) throw new Error('Scene document variableCollections are invalid.');
  const variables = new Map<string, SceneVariable>();
  const variableCollections = new Map<string, SceneVariableCollection>();
  for (const collection of document.variableCollections) {
    assertIdentifier(collection.id, 'variableCollection.id');
    if (ids.has(collection.id)) throw new Error(`Duplicate scene id: ${collection.id}`);
    ids.add(collection.id);
    if (!collection.name.trim()) throw new Error(`Variable collection ${collection.id} needs a name.`);
    if (variableCollections.has(collection.id)) throw new Error(`Duplicate variable collection: ${collection.id}`);
    variableCollections.set(collection.id, collection);
    if (!Array.isArray(collection.modes)) throw new Error(`Variable collection ${collection.id} modes are invalid.`);
    for (const mode of collection.modes) {
      assertIdentifier(mode.id, `variableCollection.${collection.id}.mode.id`);
      if (!mode.name.trim()) throw new Error(`Variable mode ${mode.id} needs a name.`);
    }
    const modeIds = new Set(collection.modes.map((mode) => mode.id));
    if (modeIds.size !== collection.modes.length || modeIds.size === 0) throw new Error(`Variable collection ${collection.id} modes are invalid.`);
    if (!Array.isArray(collection.variables)) throw new Error(`Variable collection ${collection.id} variables are invalid.`);
    for (const variable of collection.variables) {
      assertIdentifier(variable.id, 'variable.id');
      if (ids.has(variable.id)) throw new Error(`Duplicate scene id: ${variable.id}`);
      ids.add(variable.id);
      if (!variable.name.trim()) throw new Error(`Variable ${variable.id} needs a name.`);
      if (!variableTypes.has(variable.type)) throw new Error(`Variable ${variable.id} type is invalid.`);
      if (!variable.valuesByMode || typeof variable.valuesByMode !== 'object' || Array.isArray(variable.valuesByMode)) throw new Error(`Variable ${variable.id} values are invalid.`);
      if (variable.aliasByMode !== undefined && (!variable.aliasByMode || typeof variable.aliasByMode !== 'object' || Array.isArray(variable.aliasByMode))) {
        throw new Error(`Variable ${variable.id} aliases are invalid.`);
      }
      const valueModeIds = Object.keys(variable.valuesByMode);
      const aliasModeIds = Object.keys(variable.aliasByMode ?? {});
      for (const modeId of [...valueModeIds, ...aliasModeIds]) if (!modeIds.has(modeId)) throw new Error(`Variable ${variable.id} references an unknown mode.`);
      for (const modeId of modeIds) {
        const hasValue = Object.hasOwn(variable.valuesByMode, modeId);
        const hasAlias = Object.hasOwn(variable.aliasByMode ?? {}, modeId);
        if (hasValue === hasAlias) throw new Error(`Variable ${variable.id} must define exactly one value or alias for mode ${modeId}.`);
        if (hasValue) assertVariableValue(variable.valuesByMode[modeId], variable.type, `Variable ${variable.id} mode ${modeId}`);
      }
      variables.set(variable.id, variable);
    }
  }
  const aliasEdges = new Map<string, Set<string>>();
  for (const variable of variables.values()) {
    for (const targetId of Object.values(variable.aliasByMode ?? {})) {
      const target = variables.get(targetId);
      if (!target) throw new Error(`Variable ${variable.id} aliases unknown variable ${targetId}.`);
      if (target.type !== variable.type) throw new Error(`Variable ${variable.id} cannot alias ${target.type} variable ${targetId}.`);
      const edges = aliasEdges.get(variable.id) ?? new Set<string>();
      edges.add(targetId);
      aliasEdges.set(variable.id, edges);
    }
  }
  const visiting = new Set<string>();
  const visited = new Set<string>();
  function visitAlias(variableId: string, chain: string[]): void {
    if (visiting.has(variableId)) throw new Error(`Variable alias cycle: ${[...chain, variableId].join(' -> ')}`);
    if (visited.has(variableId)) return;
    visiting.add(variableId);
    for (const targetId of aliasEdges.get(variableId) ?? []) visitAlias(targetId, [...chain, variableId]);
    visiting.delete(variableId);
    visited.add(variableId);
  }
  for (const variableId of variables.keys()) visitAlias(variableId, []);
  const sceneIndex = indexSceneDocumentUnchecked(document);
  const pageIds = new Set(document.pages.map((page) => page.id));
  for (const entry of sceneIndex.values()) {
    if (entry.node.prototypeLink && !pageIds.has(entry.node.prototypeLink.targetPageId)) {
      throw new Error(`Node ${entry.node.id} prototype link references unknown page ${entry.node.prototypeLink.targetPageId}.`);
    }
  }
  for (const entry of sceneIndex.values()) assertVariableBindings(entry.node, variables);
  const componentMains = new Map<string, SceneComponentMainNode>();
  for (const entry of sceneIndex.values()) if (entry.node.type === 'component-main') componentMains.set(entry.node.id, entry.node);
  for (const main of componentMains.values()) assertComponentDefinitions(main, componentMains);
  for (const entry of sceneIndex.values()) {
    if (entry.node.type === 'component-set') {
      const variantProperties = entry.node.variantProperties;
      if (!Array.isArray(variantProperties) || variantProperties.length === 0 || new Set(variantProperties).size !== variantProperties.length
        || variantProperties.some((property) => typeof property !== 'string' || !property)) {
        throw new Error(`Component set ${entry.node.id} variant properties are invalid.`);
      }
      for (const main of entry.node.children) {
        for (const property of variantProperties) {
          if (main.propertyDefinitions[property]?.type !== 'variant') throw new Error(`Component set ${entry.node.id} main ${main.id} lacks variant property ${property}.`);
        }
      }
    }
    if (entry.node.type === 'component-instance') {
      assertIdentifier(entry.node.mainComponentId, `componentInstance.${entry.node.id}.mainComponentId`);
      const main = componentMains.get(entry.node.mainComponentId);
      if (!main) throw new Error(`Component instance ${entry.node.id} references unknown main component ${entry.node.mainComponentId}.`);
      assertComponentInstance(entry.node, main, sceneIndex, componentMains);
    }
  }
  if (!Array.isArray(document.responsiveRules)) throw new Error('Scene document responsiveRules are invalid.');
  for (const rule of document.responsiveRules) {
    assertIdentifier(rule.id, 'responsiveRule.id');
    if (ids.has(rule.id)) throw new Error(`Duplicate scene id: ${rule.id}`);
    ids.add(rule.id);
    if (typeof rule.name !== 'string' || !rule.name.trim()) throw new Error(`Responsive rule ${rule.id} needs a name.`);
    if (rule.minWidth !== undefined) assertFinite(rule.minWidth, `responsiveRule.${rule.id}.minWidth`, 0);
    if (rule.maxWidth !== undefined) assertFinite(rule.maxWidth, `responsiveRule.${rule.id}.maxWidth`, 0);
    if (rule.minWidth !== undefined && rule.maxWidth !== undefined && rule.minWidth >= rule.maxWidth) throw new Error(`Responsive rule ${rule.id} width range is invalid.`);
    if (!rule.variableModes || typeof rule.variableModes !== 'object' || Array.isArray(rule.variableModes)) throw new Error(`Responsive rule ${rule.id} variableModes are invalid.`);
    for (const [collectionId, modeId] of Object.entries(rule.variableModes)) {
      const collection = variableCollections.get(collectionId);
      if (!collection) throw new Error(`Responsive rule ${rule.id} references unknown variable collection ${collectionId}.`);
      if (!collection.modes.some((mode) => mode.id === modeId)) throw new Error(`Responsive rule ${rule.id} references unknown mode ${modeId}.`);
    }
    if (!Array.isArray(rule.nodeOverrides)) throw new Error(`Responsive rule ${rule.id} nodeOverrides are invalid.`);
    if (Object.keys(rule.variableModes).length === 0 && rule.nodeOverrides.length === 0) throw new Error(`Responsive rule ${rule.id} has no effect.`);
    const overriddenNodeIds = new Set<string>();
    for (const override of rule.nodeOverrides) {
      if (!override || typeof override !== 'object' || typeof override.nodeId !== 'string') throw new Error(`Responsive rule ${rule.id} has an invalid node override.`);
      if (overriddenNodeIds.has(override.nodeId)) throw new Error(`Responsive rule ${rule.id} overrides node ${override.nodeId} more than once.`);
      overriddenNodeIds.add(override.nodeId);
      const entry = sceneIndex.get(override.nodeId);
      if (!entry) throw new Error(`Responsive rule ${rule.id} references unknown node ${override.nodeId}.`);
      if (override.visible !== undefined && typeof override.visible !== 'boolean') throw new Error(`Responsive rule ${rule.id} visibility override is invalid.`);
      if (override.layout !== undefined) {
        if (!override.layout || typeof override.layout !== 'object' || Array.isArray(override.layout)) throw new Error(`Responsive rule ${rule.id} layout override is invalid.`);
        assertLayout(mergeSceneLayout(entry.node.layout, override.layout), `responsiveRule.${rule.id}.node.${override.nodeId}.layout`);
      }
      if (override.childOrder !== undefined) {
        if (!isSceneContainer(entry.node)) throw new Error(`Responsive rule ${rule.id} cannot order children of ${override.nodeId}.`);
        const childIds = entry.node.children.map((child) => child.id);
        if (!Array.isArray(override.childOrder) || override.childOrder.length !== childIds.length
          || new Set(override.childOrder).size !== childIds.length || override.childOrder.some((id) => !childIds.includes(id))) {
          throw new Error(`Responsive rule ${rule.id} child order for ${override.nodeId} is invalid.`);
        }
      }
      if (override.visible === undefined && override.layout === undefined && override.childOrder === undefined) {
        throw new Error(`Responsive rule ${rule.id} override for ${override.nodeId} has no effect.`);
      }
    }
  }
}

function indexSceneDocumentUnchecked(document: SceneDocument): Map<string, SceneIndexEntry> {
  const index = new Map<string, SceneIndexEntry>();
  const ids = new Set<string>([document.documentId, ...document.pages.map((page) => page.id)]);
  const objects = new WeakSet<object>();
  for (const [pageIndex, page] of document.pages.entries()) {
    for (const [childIndex, child] of page.children.entries()) visitNode(child, page.id, page.id, [pageIndex, childIndex], ids, objects, index);
  }
  return index;
}

export function indexSceneDocument(document: SceneDocument): Map<string, SceneIndexEntry> {
  assertSceneDocument(document);
  return indexSceneDocumentUnchecked(document);
}

const defaultLayout = (): SceneLayout => ({
  mode: 'free',
  padding: { top: 0, right: 0, bottom: 0, left: 0 },
  gap: { row: 0, column: 0 },
  sizingX: 'fixed',
  sizingY: 'fixed',
  position: 'flow',
  clipContent: false
});

const defaultAppearance = (): SceneAppearance => ({
  opacity: 1,
  blendMode: 'normal',
  fills: [],
  strokes: [],
  effects: [],
  radius: { topLeft: 0, topRight: 0, bottomRight: 0, bottomLeft: 0 }
});

function randomSceneIdPart(): string {
  const cryptoApi = globalThis.crypto;
  if (cryptoApi && typeof cryptoApi.randomUUID === 'function') return cryptoApi.randomUUID().slice(0, 8);
  return `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`.slice(0, 8);
}

export function createSceneNodeBase(type: SceneNodeType, name: string, frame: SceneRect, creator: SceneCreator = 'human'): SceneNodeBase {
  const now = new Date().toISOString();
  return {
    id: `${type}-${randomSceneIdPart()}`,
    type,
    name,
    visible: true,
    locked: false,
    frame: { ...frame },
    transform: { rotation: 0, scaleX: 1, scaleY: 1, skewX: 0, skewY: 0 },
    layout: defaultLayout(),
    appearance: defaultAppearance(),
    variableBindings: {},
    annotations: [],
    aiPolicy: { editable: true, lockedFields: [] },
    createdBy: creator,
    updatedBy: creator,
    createdAt: now,
    updatedAt: now
  };
}

export function createBlankSceneDocument(name = '未命名网站'): SceneDocument {
  const now = new Date().toISOString();
  return {
    schemaVersion: 2,
    documentId: `scene-${randomSceneIdPart()}`,
    revision: 0,
    name,
    pages: [{ id: `page-${randomSceneIdPart()}`, name: 'Page 1', children: [] }],
    variableCollections: [],
    responsiveRules: [],
    createdAt: now,
    updatedAt: now
  };
}
