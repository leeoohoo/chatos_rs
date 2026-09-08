import {
  assertSceneDocument,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneDocument,
  type SceneLayout,
  type SceneNode,
  type SceneRect
} from './scene-schema.js';
import { expandSceneGridTracks, resolveSceneGridTracks } from './grid-tracks.js';
import { placeSceneGridItems, type SceneGridPlacementResult } from './grid-placement.js';
import { resolveResponsiveScene } from './responsive-scene.js';
import type { SceneVariableModeSelection } from './scene-variables.js';

export interface SceneLayoutSolveOptions {
  rootNodeId: string;
  viewportWidth: number;
  viewportHeight?: number;
  textMeasurer?: SceneTextMeasurer;
}

export interface SceneTextMeasureRequest {
  nodeId: string;
  content: string;
  availableWidth?: number;
  typography: SceneNode['appearance']['typography'];
}

export interface SceneTextMeasureResult {
  width: number;
  height: number;
}

export type SceneTextMeasurer = (request: SceneTextMeasureRequest) => SceneTextMeasureResult | undefined;

export interface SolvedSceneBox extends SceneRect {
  nodeId: string;
  parentId: string;
  pageId: string;
  contentWidth: number;
  contentHeight: number;
  overflowX: boolean;
  overflowY: boolean;
}

export interface SceneLayoutDiagnostic {
  nodeId: string;
  severity: 'warning' | 'error';
  code: 'overflow-x' | 'overflow-y' | 'fill-in-hug-axis' | 'grid-placement-collision' | 'grid-placement-out-of-bounds';
  message: string;
}

export interface SolvedSceneLayout {
  documentId: string;
  revision: number;
  rootNodeId: string;
  viewportWidth: number;
  viewportHeight?: number;
  activeResponsiveRuleIds: string[];
  variableModes: SceneVariableModeSelection;
  boxes: Map<string, SolvedSceneBox>;
  diagnostics: SceneLayoutDiagnostic[];
}

interface Size {
  width: number;
  height: number;
}

interface ChildMeasurement {
  node: SceneNode;
  natural: Size;
  width: number;
  height: number;
}

interface LayoutContext {
  boxes: Map<string, SolvedSceneBox>;
  diagnostics: SceneLayoutDiagnostic[];
  pageId: string;
  textMeasurer?: SceneTextMeasurer;
}

interface FlowLine {
  children: ChildMeasurement[];
  mainSize: number;
  crossSize: number;
}

function finiteDimension(value: number | undefined, label: string): number | undefined {
  if (value === undefined) return undefined;
  if (!Number.isFinite(value) || value < 0) throw new Error(`${label} is invalid.`);
  return value;
}

function clampDimension(value: number, minimum: number | undefined, maximum: number | undefined): number {
  return Math.max(minimum ?? 0, Math.min(maximum ?? Number.POSITIVE_INFINITY, value));
}

function clampWidth(node: SceneNode, width: number): number {
  return clampDimension(width, node.layout.minWidth, node.layout.maxWidth);
}

function clampHeight(node: SceneNode, height: number): number {
  return clampDimension(height, node.layout.minHeight, node.layout.maxHeight);
}

function lineHeight(node: SceneNode): number {
  const typography = node.appearance.typography;
  const fontSize = typography?.fontSize ?? 16;
  const configured = typography?.lineHeight ?? 1.4;
  return configured <= 4 ? fontSize * configured : configured;
}

function isFullWidthCodePoint(codePoint: number): boolean {
  return codePoint >= 0x1100 && (
    codePoint <= 0x115f
    || codePoint === 0x2329
    || codePoint === 0x232a
    || (codePoint >= 0x2e80 && codePoint <= 0xa4cf && codePoint !== 0x303f)
    || (codePoint >= 0xac00 && codePoint <= 0xd7a3)
    || (codePoint >= 0xf900 && codePoint <= 0xfaff)
    || (codePoint >= 0xfe10 && codePoint <= 0xfe19)
    || (codePoint >= 0xfe30 && codePoint <= 0xfe6f)
    || (codePoint >= 0xff00 && codePoint <= 0xff60)
    || (codePoint >= 0xffe0 && codePoint <= 0xffe6)
    || (codePoint >= 0x1f300 && codePoint <= 0x1faff)
    || (codePoint >= 0x20000 && codePoint <= 0x3fffd)
  );
}

function characterAdvance(character: string, fontSize: number, letterSpacing: number): number {
  const codePoint = character.codePointAt(0) ?? 0;
  if (isFullWidthCodePoint(codePoint)) return fontSize + letterSpacing;
  if (/[—–―]/u.test(character)) return fontSize + letterSpacing;
  if (/\s/u.test(character)) return fontSize * 0.33 + letterSpacing;
  if (/[ilI|.,'`:;]/u.test(character)) return fontSize * 0.3 + letterSpacing;
  if (/[¥￥$€£₩₹]/u.test(character)) return fontSize + letterSpacing;
  if (/[MW@#%&]/u.test(character)) return fontSize * 0.82 + letterSpacing;
  if (/[A-Z]/u.test(character)) return fontSize * 0.62 + letterSpacing;
  if (/[0-9]/u.test(character)) return fontSize * 0.56 + letterSpacing;
  return fontSize * 0.54 + letterSpacing;
}

function estimatedTextWidth(value: string, fontSize: number, letterSpacing: number): number {
  return [...value].reduce((total, character) => total + characterAdvance(character, fontSize, letterSpacing), 0);
}

function textNaturalSize(node: Extract<SceneNode, { type: 'text' }>, availableWidth?: number, textMeasurer?: SceneTextMeasurer): Size {
  const measured = textMeasurer?.({
    nodeId: node.id,
    content: node.content,
    availableWidth,
    typography: node.appearance.typography ? structuredClone(node.appearance.typography) : undefined
  });
  if (measured !== undefined) {
    if (!Number.isFinite(measured.width) || measured.width < 0 || !Number.isFinite(measured.height) || measured.height < 0) {
      throw new Error(`Text measurer returned an invalid size for ${node.id}.`);
    }
    return {
      width: clampWidth(node, Math.min(measured.width, availableWidth ?? Number.POSITIVE_INFINITY)),
      height: clampHeight(node, measured.height)
    };
  }
  const fontSize = node.appearance.typography?.fontSize ?? 16;
  const letterSpacing = node.appearance.typography?.letterSpacing ?? 0;
  const widthSafetyFactor = 1.1;
  const paragraphs = node.content.split('\n');
  const widestCharacter = Math.max(1, ...[...node.content].map((character) => characterAdvance(character, fontSize, letterSpacing) * widthSafetyFactor));
  const paragraphWidths = paragraphs.map((paragraph) => Math.max(widestCharacter, estimatedTextWidth(paragraph, fontSize, letterSpacing) * widthSafetyFactor));
  const unwrappedWidth = Math.max(widestCharacter, ...paragraphWidths);
  const widthLimit = Math.max(widestCharacter, Math.min(availableWidth ?? Number.POSITIVE_INFINITY, node.layout.maxWidth ?? Number.POSITIVE_INFINITY));
  const width = Math.min(unwrappedWidth, widthLimit);
  const lines = paragraphWidths.reduce((total, paragraphWidth) => total + Math.max(1, Math.ceil(paragraphWidth / width)), 0);
  const configuredLineHeight = lineHeight(node);
  const glyphOverflowAllowance = Math.max(0, fontSize * 1.1 - configuredLineHeight);
  return { width: clampWidth(node, width), height: clampHeight(node, lines * configuredLineHeight + glyphOverflowAllowance) };
}

function childNodes(node: SceneNode, includeHidden = false): SceneNode[] {
  const children = isSceneContainer(node) ? node.children : isSceneSlotContainer(node) ? Object.values(node.slots).flat() : [];
  return includeHidden ? children : children.filter((child) => child.visible);
}

function fixedOrNaturalWidth(node: SceneNode, natural: Size): number {
  if (node.layout.sizingX === 'fixed') return clampWidth(node, node.frame.width);
  return clampWidth(node, natural.width);
}

function fixedOrNaturalHeight(node: SceneNode, natural: Size): number {
  if (node.layout.sizingY === 'fixed') return clampHeight(node, node.frame.height);
  return clampHeight(node, natural.height);
}

function remeasureHeightAtWidth(measurement: ChildMeasurement, width: number, textMeasurer?: SceneTextMeasurer): void {
  if (measurement.node.layout.sizingY === 'fixed') return;
  const natural = measureNatural(measurement.node, width, textMeasurer);
  measurement.natural = natural;
  measurement.height = clampHeight(measurement.node, natural.height);
}

function measureNaturalGrid(node: SceneNode, availableWidth: number | undefined, textMeasurer?: SceneTextMeasurer): Size {
  const grid = node.layout.grid!;
  const padding = node.layout.padding;
  const horizontalPadding = padding.left + padding.right;
  const verticalPadding = padding.top + padding.bottom;
  const innerWidth = Math.max(0, (availableWidth ?? node.frame.width) - horizontalPadding);
  const flowChildren = childNodes(node).filter((child) => child.layout.position === 'flow');
  const measurements = new Map<string, ChildMeasurement>();
  for (const child of flowChildren) {
    const natural = measureNatural(child, innerWidth, textMeasurer);
    measurements.set(child.id, {
      node: child,
      natural,
      width: fixedOrNaturalWidth(child, natural),
      height: fixedOrNaturalHeight(child, natural)
    });
  }
  const columns = expandSceneGridTracks(grid.columns, innerWidth, node.layout.gap.column, flowChildren.length);
  const configuredRows = grid.rows.length === 0 ? [] : expandSceneGridTracks(grid.rows, node.frame.height, node.layout.gap.row, flowChildren.length);
  const placements = placeSceneGridItems(flowChildren.map((child) => ({
    nodeId: child.id,
    columnStart: child.layout.gridPlacement?.columnStart,
    rowStart: child.layout.gridPlacement?.rowStart,
    columnSpan: child.layout.gridPlacement?.columnSpan,
    rowSpan: child.layout.gridPlacement?.rowSpan
  })), columns.length, grid.autoFlow, configuredRows.length);
  const columnSizes = resolveSceneGridTracks(columns, innerWidth, node.layout.gap.column, gridContentSizes(placements, measurements, 'column', columns.length));
  for (const placement of placements) {
    if (placement.outOfBounds) continue;
    const measurement = measurements.get(placement.nodeId)!;
    const cellWidth = spannedSize(columnSizes, placement.column, placement.columnSpan, node.layout.gap.column);
    if (measurement.node.layout.sizingX === 'fill') measurement.width = clampWidth(measurement.node, cellWidth);
    remeasureHeightAtWidth(measurement, measurement.width, textMeasurer);
  }
  const rowCount = Math.max(configuredRows.length, ...placements.map((placement) => placement.row + placement.rowSpan), 1);
  const rows = [...configuredRows];
  while (rows.length < rowCount) rows.push({ source: 'auto', track: { kind: 'auto' } });
  const rowContent = gridContentSizes(placements, measurements, 'row', rows.length);
  const rowContentHeight = rowContent.reduce((total, size) => total + size, 0) + Math.max(0, rows.length - 1) * node.layout.gap.row;
  const rowSizes = resolveSceneGridTracks(rows, rowContentHeight, node.layout.gap.row, rowContent);
  const contentWidth = columnSizes.reduce((total, size) => total + size, 0) + Math.max(0, columnSizes.length - 1) * node.layout.gap.column;
  const contentHeight = rowSizes.reduce((total, size) => total + size, 0) + Math.max(0, rowSizes.length - 1) * node.layout.gap.row;
  return {
    width: clampWidth(node, contentWidth + horizontalPadding),
    height: clampHeight(node, contentHeight + verticalPadding)
  };
}

function measureNatural(node: SceneNode, availableWidth?: number, textMeasurer?: SceneTextMeasurer): Size {
  if (node.type === 'text') return textNaturalSize(node, availableWidth, textMeasurer);
  if (node.type === 'media' && node.intrinsicSize) {
    const width = Math.min(node.intrinsicSize.width, availableWidth ?? Number.POSITIVE_INFINITY);
    const height = node.preserveAspectRatio ? width * node.intrinsicSize.height / node.intrinsicSize.width : node.intrinsicSize.height;
    return { width: clampWidth(node, width), height: clampHeight(node, height) };
  }
  const children = childNodes(node);
  if (children.length === 0) return { width: clampWidth(node, node.frame.width), height: clampHeight(node, node.frame.height) };
  const padding = node.layout.padding;
  const childAvailableWidth = availableWidth === undefined
    ? undefined
    : Math.max(0, availableWidth - padding.left - padding.right);
  if (node.layout.mode === 'grid') return measureNaturalGrid(node, availableWidth, textMeasurer);
  if (node.layout.mode === 'auto') {
    const flow = children.filter((child) => child.layout.position === 'flow');
    const measurements = flow.map((child) => {
      const natural = measureNatural(child, childAvailableWidth, textMeasurer);
      return { width: fixedOrNaturalWidth(child, natural), height: fixedOrNaturalHeight(child, natural) };
    });
    if (node.layout.direction === 'horizontal') {
      const width = padding.left + padding.right + measurements.reduce((total, size) => total + size.width, 0)
        + Math.max(0, measurements.length - 1) * node.layout.gap.column;
      const height = padding.top + padding.bottom + Math.max(0, ...measurements.map((size) => size.height));
      return { width: clampWidth(node, width), height: clampHeight(node, height) };
    }
    const width = padding.left + padding.right + Math.max(0, ...measurements.map((size) => size.width));
    const height = padding.top + padding.bottom + measurements.reduce((total, size) => total + size.height, 0)
      + Math.max(0, measurements.length - 1) * node.layout.gap.row;
    return { width: clampWidth(node, width), height: clampHeight(node, height) };
  }
  const bounds = children.map((child) => {
    const natural = measureNatural(child, childAvailableWidth, textMeasurer);
    return {
      right: child.frame.x + fixedOrNaturalWidth(child, natural),
      bottom: child.frame.y + fixedOrNaturalHeight(child, natural)
    };
  });
  return {
    width: clampWidth(node, padding.left + padding.right + Math.max(0, ...bounds.map((bound) => bound.right))),
    height: clampHeight(node, padding.top + padding.bottom + Math.max(0, ...bounds.map((bound) => bound.bottom)))
  };
}

function axisSizes(direction: 'horizontal' | 'vertical', size: Size): { main: number; cross: number } {
  return direction === 'horizontal' ? { main: size.width, cross: size.height } : { main: size.height, cross: size.width };
}

function fromAxisSizes(direction: 'horizontal' | 'vertical', main: number, cross: number): Size {
  return direction === 'horizontal' ? { width: main, height: cross } : { width: cross, height: main };
}

function mainGap(layout: SceneLayout): number {
  return layout.direction === 'horizontal' ? layout.gap.column : layout.gap.row;
}

function crossGap(layout: SceneLayout): number {
  return layout.direction === 'horizontal' ? layout.gap.row : layout.gap.column;
}

function distributeFill(children: ChildMeasurement[], direction: 'horizontal' | 'vertical', availableMain: number, gap: number): void {
  const fillChildren = children.filter((child) => direction === 'horizontal' ? child.node.layout.sizingX === 'fill' : child.node.layout.sizingY === 'fill');
  if (fillChildren.length === 0) return;
  const occupied = children.filter((child) => !fillChildren.includes(child)).reduce((total, child) => total + axisSizes(direction, child).main, 0)
    + Math.max(0, children.length - 1) * gap;
  const share = Math.max(0, availableMain - occupied) / fillChildren.length;
  for (const child of fillChildren) {
    if (direction === 'horizontal') child.width = clampWidth(child.node, share);
    else child.height = clampHeight(child.node, share);
  }
}

function buildLines(children: ChildMeasurement[], direction: 'horizontal' | 'vertical', availableMain: number, gap: number, wrap: boolean): FlowLine[] {
  if (!wrap || !Number.isFinite(availableMain)) {
    return [{
      children,
      mainSize: children.reduce((total, child) => total + axisSizes(direction, child).main, 0) + Math.max(0, children.length - 1) * gap,
      crossSize: Math.max(0, ...children.map((child) => axisSizes(direction, child).cross))
    }];
  }
  const lines: FlowLine[] = [];
  let line: FlowLine = { children: [], mainSize: 0, crossSize: 0 };
  for (const child of children) {
    const size = axisSizes(direction, child);
    const nextMain = line.children.length === 0 ? size.main : line.mainSize + gap + size.main;
    if (line.children.length > 0 && nextMain > availableMain) {
      lines.push(line);
      line = { children: [], mainSize: 0, crossSize: 0 };
    }
    line.children.push(child);
    line.mainSize = line.children.length === 1 ? size.main : line.mainSize + gap + size.main;
    line.crossSize = Math.max(line.crossSize, size.cross);
  }
  if (line.children.length > 0 || lines.length === 0) lines.push(line);
  return lines;
}

function justifyOffset(layout: SceneLayout, freeSpace: number, itemCount: number): { start: number; gapExtra: number } {
  const free = Math.max(0, freeSpace);
  switch (layout.justifyContent) {
    case 'center': return { start: free / 2, gapExtra: 0 };
    case 'end': return { start: free, gapExtra: 0 };
    case 'between': return { start: 0, gapExtra: itemCount > 1 ? free / (itemCount - 1) : 0 };
    case 'around': return { start: itemCount > 0 ? free / itemCount / 2 : 0, gapExtra: itemCount > 0 ? free / itemCount : 0 };
    case 'evenly': return { start: itemCount > 0 ? free / (itemCount + 1) : 0, gapExtra: itemCount > 0 ? free / (itemCount + 1) : 0 };
    default: return { start: 0, gapExtra: 0 };
  }
}

function crossOffset(layout: SceneLayout, freeSpace: number): number {
  if (layout.alignItems === 'center') return Math.max(0, freeSpace) / 2;
  if (layout.alignItems === 'end') return Math.max(0, freeSpace);
  return 0;
}

function solveConstraintAxis(
  start: number,
  size: number,
  referenceParentSize: number,
  solvedParentSize: number,
  constraint: 'left' | 'center' | 'right' | 'top' | 'bottom' | 'stretch' | 'scale'
): { start: number; size: number } {
  if (constraint === 'right' || constraint === 'bottom') {
    return { start: solvedParentSize - (referenceParentSize - start - size) - size, size };
  }
  if (constraint === 'center') {
    const centerOffset = start + size / 2 - referenceParentSize / 2;
    return { start: solvedParentSize / 2 + centerOffset - size / 2, size };
  }
  if (constraint === 'stretch') {
    const endMargin = referenceParentSize - start - size;
    return { start, size: Math.max(0, solvedParentSize - start - endMargin) };
  }
  if (constraint === 'scale') {
    const ratio = referenceParentSize > 0 ? solvedParentSize / referenceParentSize : 1;
    return { start: start * ratio, size: size * ratio };
  }
  return { start, size };
}

function constrainedChildRect(
  parent: SceneNode,
  child: SceneNode,
  solvedParentWidth: number,
  solvedParentHeight: number,
  referenceParentWidth = parent.frame.width,
  referenceParentHeight = parent.frame.height,
  textMeasurer?: SceneTextMeasurer,
  natural = measureNatural(child, solvedParentWidth, textMeasurer)
): SceneRect {
  const constraints = child.layout.constraints ?? { horizontal: 'left', vertical: 'top' };
  const designWidth = child.layout.sizingX === 'hug' ? natural.width : child.frame.width;
  const designHeight = child.layout.sizingY === 'hug' ? natural.height : child.frame.height;
  const horizontal = child.layout.sizingX === 'fill'
    ? { start: 0, size: solvedParentWidth }
    : solveConstraintAxis(child.frame.x, designWidth, referenceParentWidth, solvedParentWidth, constraints.horizontal);
  const vertical = child.layout.sizingY === 'fill'
    ? { start: 0, size: solvedParentHeight }
    : solveConstraintAxis(child.frame.y, designHeight, referenceParentHeight, solvedParentHeight, constraints.vertical);
  return {
    x: horizontal.start,
    y: vertical.start,
    width: clampWidth(child, horizontal.size),
    height: clampHeight(child, vertical.size)
  };
}

function trackOffsets(sizes: number[], gap: number): number[] {
  const offsets: number[] = [];
  let cursor = 0;
  for (const size of sizes) {
    offsets.push(cursor);
    cursor += size + gap;
  }
  return offsets;
}

function spannedSize(sizes: number[], start: number, span: number, gap: number): number {
  return sizes.slice(start, start + span).reduce((total, size) => total + size, 0) + Math.max(0, span - 1) * gap;
}

function gridContentSizes(
  placements: SceneGridPlacementResult[],
  measurements: Map<string, ChildMeasurement>,
  axis: 'column' | 'row',
  count: number
): number[] {
  const sizes = Array.from({ length: count }, () => 0);
  for (const placement of placements) {
    const span = axis === 'column' ? placement.columnSpan : placement.rowSpan;
    if (span !== 1) continue;
    const index = axis === 'column' ? placement.column : placement.row;
    const measurement = measurements.get(placement.nodeId)!;
    sizes[index] = Math.max(sizes[index] ?? 0, axis === 'column' ? measurement.width : measurement.height);
  }
  return sizes;
}

function solveGridChildren(
  node: SceneNode,
  parentId: string,
  x: number,
  y: number,
  width: number,
  height: number,
  assignedWidth: number | undefined,
  assignedHeight: number | undefined,
  context: LayoutContext
): { width: number; height: number; contentWidth: number; contentHeight: number } {
  const grid = node.layout.grid!;
  const padding = node.layout.padding;
  const horizontalPadding = padding.left + padding.right;
  const verticalPadding = padding.top + padding.bottom;
  const innerWidth = Math.max(0, width - horizontalPadding);
  const innerHeight = Math.max(0, height - verticalPadding);
  const flowChildren = childNodes(node).filter((child) => child.layout.position === 'flow');
  const measurements = new Map<string, ChildMeasurement>();
  for (const child of flowChildren) {
    const natural = measureNatural(child, innerWidth, context.textMeasurer);
    measurements.set(child.id, {
      node: child,
      natural,
      width: fixedOrNaturalWidth(child, natural),
      height: fixedOrNaturalHeight(child, natural)
    });
  }
  const columns = expandSceneGridTracks(grid.columns, innerWidth, node.layout.gap.column, flowChildren.length);
  const preliminaryRows = grid.rows.length === 0 ? [] : expandSceneGridTracks(grid.rows, innerHeight, node.layout.gap.row, flowChildren.length);
  const placements = placeSceneGridItems(flowChildren.map((child) => ({
    nodeId: child.id,
    columnStart: child.layout.gridPlacement?.columnStart,
    rowStart: child.layout.gridPlacement?.rowStart,
    columnSpan: child.layout.gridPlacement?.columnSpan,
    rowSpan: child.layout.gridPlacement?.rowSpan
  })), columns.length, grid.autoFlow, preliminaryRows.length);
  for (const placement of placements) {
    if (placement.collision) context.diagnostics.push({
      nodeId: placement.nodeId,
      severity: 'error',
      code: 'grid-placement-collision',
      message: `Grid item ${placement.nodeId} overlaps an occupied cell.`
    });
    if (placement.outOfBounds) context.diagnostics.push({
      nodeId: placement.nodeId,
      severity: 'error',
      code: 'grid-placement-out-of-bounds',
      message: `Grid item ${placement.nodeId} exceeds the available columns.`
    });
  }
  const rowCount = Math.max(preliminaryRows.length, ...placements.map((placement) => placement.row + placement.rowSpan), 1);
  const rows = [...preliminaryRows];
  while (rows.length < rowCount) rows.push({ source: 'auto', track: { kind: 'auto' } });
  const columnSizes = resolveSceneGridTracks(columns, innerWidth, node.layout.gap.column, gridContentSizes(placements, measurements, 'column', columns.length));
  for (const placement of placements) {
    if (placement.outOfBounds) continue;
    const measurement = measurements.get(placement.nodeId)!;
    const cellWidth = spannedSize(columnSizes, placement.column, placement.columnSpan, node.layout.gap.column);
    if (measurement.node.layout.sizingX === 'fill') measurement.width = clampWidth(measurement.node, cellWidth);
    remeasureHeightAtWidth(measurement, measurement.width, context.textMeasurer);
  }
  const naturalRowContent = gridContentSizes(placements, measurements, 'row', rows.length);
  const naturalRowsHeight = naturalRowContent.reduce((total, size) => total + size, 0) + Math.max(0, rows.length - 1) * node.layout.gap.row;
  const rowAvailable = node.layout.sizingY === 'hug' && assignedHeight === undefined ? naturalRowsHeight : innerHeight;
  const rowSizes = resolveSceneGridTracks(rows, rowAvailable, node.layout.gap.row, naturalRowContent);
  const contentWidth = columnSizes.reduce((total, size) => total + size, 0) + Math.max(0, columnSizes.length - 1) * node.layout.gap.column;
  const contentHeight = rowSizes.reduce((total, size) => total + size, 0) + Math.max(0, rowSizes.length - 1) * node.layout.gap.row;
  if (node.layout.sizingX === 'hug' && assignedWidth === undefined) width = clampWidth(node, contentWidth + horizontalPadding);
  if (node.layout.sizingY === 'hug' && assignedHeight === undefined) height = clampHeight(node, contentHeight + verticalPadding);
  const columnOffsets = trackOffsets(columnSizes, node.layout.gap.column);
  const rowOffsets = trackOffsets(rowSizes, node.layout.gap.row);
  for (const placement of placements) {
    if (placement.outOfBounds) continue;
    const measurement = measurements.get(placement.nodeId)!;
    const cellWidth = spannedSize(columnSizes, placement.column, placement.columnSpan, node.layout.gap.column);
    const cellHeight = spannedSize(rowSizes, placement.row, placement.rowSpan, node.layout.gap.row);
    const childWidth = measurement.node.layout.sizingX === 'fill' ? cellWidth : Math.min(cellWidth, measurement.width);
    const childHeight = measurement.node.layout.sizingY === 'fill' ? cellHeight : Math.min(cellHeight, measurement.height);
    const childX = columnOffsets[placement.column] + crossOffset(node.layout, cellWidth - childWidth);
    const childY = rowOffsets[placement.row] + crossOffset(node.layout, cellHeight - childHeight);
    solveNode(measurement.node, parentId, x + padding.left + childX, y + padding.top + childY, childWidth, childHeight, context);
  }
  for (const child of childNodes(node).filter((candidate) => candidate.layout.position === 'absolute')) {
    const rect = constrainedChildRect(node, child, width, height, node.frame.width, node.frame.height, context.textMeasurer);
    solveNode(child, node.id, x + rect.x, y + rect.y, rect.width, rect.height, context);
  }
  return { width, height, contentWidth, contentHeight };
}

function solveNode(
  node: SceneNode,
  parentId: string,
  x: number,
  y: number,
  assignedWidth: number | undefined,
  assignedHeight: number | undefined,
  context: LayoutContext
): Size {
  const natural = measureNatural(node, assignedWidth, context.textMeasurer);
  let width = clampWidth(node, assignedWidth ?? fixedOrNaturalWidth(node, natural));
  let height = clampHeight(node, assignedHeight ?? fixedOrNaturalHeight(node, natural));
  let contentWidth = 0;
  let contentHeight = 0;

  if (node.type === 'text') {
    const measured = textNaturalSize(node, width, context.textMeasurer);
    if (node.layout.sizingX === 'hug' && assignedWidth === undefined) width = measured.width;
    if (node.layout.sizingY !== 'fixed' && assignedHeight === undefined) height = measured.height;
    contentWidth = measured.width;
    contentHeight = measured.height;
  } else if (node.type === 'media' && node.intrinsicSize) {
    const ratio = node.intrinsicSize.width / node.intrinsicSize.height;
    if (node.preserveAspectRatio && assignedWidth !== undefined && assignedHeight === undefined && node.layout.sizingY === 'hug') {
      height = clampHeight(node, width / ratio);
    } else if (node.preserveAspectRatio && assignedHeight !== undefined && assignedWidth === undefined && node.layout.sizingX === 'hug') {
      width = clampWidth(node, height * ratio);
    }
    contentWidth = width;
    contentHeight = height;
  } else if (node.layout.mode === 'grid' && childNodes(node).length > 0) {
    const gridResult = solveGridChildren(node, node.id, x, y, width, height, assignedWidth, assignedHeight, context);
    width = gridResult.width;
    height = gridResult.height;
    contentWidth = gridResult.contentWidth;
    contentHeight = gridResult.contentHeight;
  } else if (node.layout.mode === 'auto' && childNodes(node).length > 0) {
    const direction = node.layout.direction!;
    const padding = node.layout.padding;
    const horizontalPadding = padding.left + padding.right;
    const verticalPadding = padding.top + padding.bottom;
    const innerWidth = Math.max(0, width - horizontalPadding);
    const innerHeight = Math.max(0, height - verticalPadding);
    const availableMain = direction === 'horizontal' ? innerWidth : innerHeight;
    const availableCross = direction === 'horizontal' ? innerHeight : innerWidth;
    const flowChildren = childNodes(node).filter((child) => child.layout.position === 'flow');
    const measurements: ChildMeasurement[] = flowChildren.map((child) => {
      const childNatural = measureNatural(child, innerWidth, context.textMeasurer);
      return {
        node: child,
        natural: childNatural,
        width: child.layout.sizingX === 'fill' ? clampWidth(child, innerWidth) : fixedOrNaturalWidth(child, childNatural),
        height: child.layout.sizingY === 'fill' ? clampHeight(child, innerHeight) : fixedOrNaturalHeight(child, childNatural)
      };
    });
    const parentHugsMain = direction === 'horizontal' ? node.layout.sizingX === 'hug' && assignedWidth === undefined : node.layout.sizingY === 'hug' && assignedHeight === undefined;
    if (parentHugsMain) {
      const hasFill = measurements.some((child) => direction === 'horizontal' ? child.node.layout.sizingX === 'fill' : child.node.layout.sizingY === 'fill');
      if (hasFill) context.diagnostics.push({
        nodeId: node.id,
        severity: 'warning',
        code: 'fill-in-hug-axis',
        message: `Fill children in hug ${direction} axis use their natural size.`
      });
      for (const child of measurements) {
        if (direction === 'horizontal' && child.node.layout.sizingX === 'fill') child.width = child.natural.width;
        if (direction === 'vertical' && child.node.layout.sizingY === 'fill') child.height = child.natural.height;
      }
    } else {
      distributeFill(measurements, direction, availableMain, mainGap(node.layout));
    }
    for (const child of measurements) {
      if ((direction === 'vertical' || node.layout.alignItems === 'stretch') && child.node.layout.sizingX === 'fill') child.width = clampWidth(child.node, innerWidth);
      if ((direction === 'horizontal' || node.layout.alignItems === 'stretch') && child.node.layout.sizingY === 'fill') child.height = clampHeight(child.node, innerHeight);
      remeasureHeightAtWidth(child, child.width, context.textMeasurer);
    }
    let lines = buildLines(measurements, direction, availableMain, mainGap(node.layout), Boolean(node.layout.wrap) && !parentHugsMain);
    const contentMain = Math.max(0, ...lines.map((line) => line.mainSize));
    const contentCross = lines.reduce((total, line) => total + line.crossSize, 0) + Math.max(0, lines.length - 1) * crossGap(node.layout);
    const contentSize = fromAxisSizes(direction, contentMain, contentCross);
    contentWidth = contentSize.width;
    contentHeight = contentSize.height;
    if (node.layout.sizingX === 'hug' && assignedWidth === undefined) width = clampWidth(node, contentWidth + horizontalPadding);
    if (node.layout.sizingY === 'hug' && assignedHeight === undefined) height = clampHeight(node, contentHeight + verticalPadding);

    const finalInnerMain = direction === 'horizontal' ? Math.max(0, width - horizontalPadding) : Math.max(0, height - verticalPadding);
    const finalInnerCross = direction === 'horizontal' ? Math.max(0, height - verticalPadding) : Math.max(0, width - horizontalPadding);
    if (!parentHugsMain) {
      distributeFill(measurements, direction, finalInnerMain, mainGap(node.layout));
      lines = buildLines(measurements, direction, finalInnerMain, mainGap(node.layout), Boolean(node.layout.wrap));
    }
    let crossCursor = 0;
    for (const line of lines) {
      const justification = justifyOffset(node.layout, finalInnerMain - line.mainSize, line.children.length);
      let mainCursor = justification.start;
      for (const child of line.children) {
        const childAxis = axisSizes(direction, child);
        const alignmentCrossSize = lines.length === 1 && !node.layout.wrap ? finalInnerCross : line.crossSize;
        const crossFree = alignmentCrossSize - childAxis.cross;
        const childMain = mainCursor;
        const childCross = crossCursor + crossOffset(node.layout, crossFree);
        const childPosition = fromAxisSizes(direction, childMain, childCross);
        solveNode(
          child.node,
          node.id,
          x + padding.left + childPosition.width,
          y + padding.top + childPosition.height,
          child.width,
          child.height,
          context
        );
        mainCursor += childAxis.main + mainGap(node.layout) + justification.gapExtra;
      }
      crossCursor += line.crossSize + crossGap(node.layout);
    }
    for (const child of childNodes(node).filter((candidate) => candidate.layout.position === 'absolute')) {
      const rect = constrainedChildRect(node, child, width, height, node.frame.width, node.frame.height, context.textMeasurer);
      solveNode(child, node.id, x + rect.x, y + rect.y, rect.width, rect.height, context);
    }
  } else {
    const padding = node.layout.padding;
    const innerWidth = Math.max(0, width - padding.left - padding.right);
    const innerHeight = Math.max(0, height - padding.top - padding.bottom);
    const referenceInnerWidth = Math.max(0, node.frame.width - padding.left - padding.right);
    const referenceInnerHeight = Math.max(0, node.frame.height - padding.top - padding.bottom);
    for (const child of childNodes(node)) {
      const rect = constrainedChildRect(node, child, innerWidth, innerHeight, referenceInnerWidth, referenceInnerHeight, context.textMeasurer);
      solveNode(child, node.id, x + padding.left + rect.x, y + padding.top + rect.y, rect.width, rect.height, context);
      contentWidth = Math.max(contentWidth, rect.x + rect.width);
      contentHeight = Math.max(contentHeight, rect.y + rect.height);
    }
    if (node.layout.sizingX === 'hug' && assignedWidth === undefined) width = clampWidth(node, contentWidth + padding.left + padding.right);
    if (node.layout.sizingY === 'hug' && assignedHeight === undefined) height = clampHeight(node, contentHeight + padding.top + padding.bottom);
  }

  const overflowX = contentWidth > Math.max(0, width - node.layout.padding.left - node.layout.padding.right) + 0.01;
  const overflowY = contentHeight > Math.max(0, height - node.layout.padding.top - node.layout.padding.bottom) + 0.01;
  if (overflowX) context.diagnostics.push({ nodeId: node.id, severity: 'warning', code: 'overflow-x', message: `Content exceeds ${node.id} width.` });
  if (overflowY) context.diagnostics.push({ nodeId: node.id, severity: 'warning', code: 'overflow-y', message: `Content exceeds ${node.id} height.` });
  context.boxes.set(node.id, { nodeId: node.id, parentId, pageId: context.pageId, x, y, width, height, contentWidth, contentHeight, overflowX, overflowY });
  return { width, height };
}

function findRoot(document: SceneDocument, nodeId: string): { node: SceneNode; parentId: string; pageId: string } {
  function search(nodes: SceneNode[], parentId: string, pageId: string): { node: SceneNode; parentId: string; pageId: string } | undefined {
    for (const node of nodes) {
      if (node.id === nodeId) return { node, parentId, pageId };
      const found = search(childNodes(node, true), node.id, pageId);
      if (found) return found;
    }
    return undefined;
  }
  for (const page of document.pages) {
    const found = search(page.children, page.id, page.id);
    if (found) return found;
  }
  throw new Error(`Scene layout root not found: ${nodeId}`);
}

export function solveSceneLayout(document: SceneDocument, options: SceneLayoutSolveOptions): SolvedSceneLayout {
  assertSceneDocument(document);
  const viewportWidth = finiteDimension(options.viewportWidth, 'viewportWidth');
  const viewportHeight = finiteDimension(options.viewportHeight, 'viewportHeight');
  if (viewportWidth === undefined || viewportWidth === 0) throw new Error('viewportWidth must be greater than zero.');
  const responsive = resolveResponsiveScene(document, viewportWidth);
  const root = findRoot(responsive.document, options.rootNodeId);
  if (!root.node.visible) throw new Error(`Scene layout root is hidden at viewport ${viewportWidth}: ${root.node.id}`);
  const context: LayoutContext = { boxes: new Map(), diagnostics: [], pageId: root.pageId, textMeasurer: options.textMeasurer };
  solveNode(root.node, root.parentId, 0, 0, viewportWidth, viewportHeight, context);
  return {
    documentId: document.documentId,
    revision: document.revision,
    rootNodeId: root.node.id,
    viewportWidth,
    ...(viewportHeight === undefined ? {} : { viewportHeight }),
    activeResponsiveRuleIds: responsive.activeRuleIds,
    variableModes: responsive.variableModes,
    boxes: context.boxes,
    diagnostics: context.diagnostics
  };
}
