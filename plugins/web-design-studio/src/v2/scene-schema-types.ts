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
