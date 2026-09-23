import type { DiagramDocument, DiagramNode } from './schema.js';
import {
  parseSequenceActivationHandle,
  parseSequenceSlot,
  sequenceActivationSlotPercentage,
  sequenceSlotPercentage
} from './sequence.js';

function escapeXml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;');
}

function absolutePosition(document: DiagramDocument, nodeId: string): { x: number; y: number } {
  const node = document.nodes.find((candidate) => candidate.id === nodeId);
  if (!node) return { x: 0, y: 0 };
  if (!node.parentId) return node.position;
  const parent = absolutePosition(document, node.parentId);
  return { x: parent.x + node.position.x, y: parent.y + node.position.y };
}

export function renderDiagramSvg(document: DiagramDocument): string {
  const positions = document.nodes.map((node) => {
    const position = absolutePosition(document, node.id);
    const iconOnly = Boolean(node.data.icon && node.data.showLabel === false);
    const unlabeled = node.data.showLabel === false;
    const width = node.width ?? (node.data.shape === 'lifeline' ? 160 : node.data.shape === 'activation' ? 14 : node.data.shape === 'fragment' ? 620 : node.data.shape === 'lane' ? 900 : node.data.shape === 'container' ? 300 : node.data.shape === 'mindmap-root' ? 200 : node.data.shape === 'mindmap-topic' ? 150 : iconOnly ? 58 : node.data.shape === 'text' ? 120 : unlabeled && node.data.shape === 'circle' ? 72 : unlabeled && node.data.shape === 'diamond' ? 96 : unlabeled && node.data.shape === 'cylinder' ? 120 : unlabeled ? 132 : node.data.shape === 'circle' ? 104 : node.data.shape === 'diamond' ? 138 : node.data.shape === 'cylinder' ? 164 : 168);
    const height = node.height ?? (node.data.shape === 'lifeline' ? 560 : node.data.shape === 'activation' ? 120 : node.data.shape === 'fragment' ? 220 : node.data.shape === 'lane' ? 180 : node.data.shape === 'container' ? 180 : node.data.shape === 'mindmap-root' ? 64 : node.data.shape === 'mindmap-topic' ? 46 : iconOnly ? 58 : node.data.shape === 'text' ? 34 : unlabeled && node.data.shape === 'circle' ? 72 : unlabeled && node.data.shape === 'diamond' ? 72 : unlabeled && node.data.shape === 'cylinder' ? 58 : unlabeled ? 56 : node.data.shape === 'circle' ? 104 : node.data.shape === 'diamond' ? 100 : node.data.shape === 'cylinder' ? 82 : 68);
    return { node, x: position.x, y: position.y, width, height };
  });
  const minX = Math.min(0, ...positions.map((item) => item.x)) - 40;
  const minY = Math.min(0, ...positions.map((item) => item.y)) - 40;
  const maxX = Math.max(800, ...positions.map((item) => item.x + item.width)) + 40;
  const maxY = Math.max(600, ...positions.map((item) => item.y + item.height)) + 40;
  const byId = new Map(positions.map((item) => [item.node.id, item]));
  const edgeMarkup = document.edges.map((edge) => {
    const source = byId.get(edge.source);
    const target = byId.get(edge.target);
    if (!source || !target) return '';
    const start = svgHandlePoint(source, edge.sourceHandle);
    const end = svgHandlePoint(target, edge.targetHandle);
    const { x: x1, y: y1 } = start;
    const { x: x2, y: y2 } = end;
    const lineStyle = edge.data?.lineStyle ?? (edge.data?.dashed ? 'dashed' : 'solid');
    const dash = lineStyle === 'dotted' ? ' stroke-dasharray="2 5" stroke-linecap="round"' : lineStyle === 'dashed' ? ' stroke-dasharray="8 6"' : '';
    const markerId = document.kind === 'sequence'
      ? lineStyle === 'dashed' ? 'sequence-return-arrow' : 'sequence-call-arrow'
      : 'arrow';
    const markerStart = document.kind !== 'mindmap' && edge.data?.startMarker === 'arrow' ? ` marker-start="url(#${markerId})"` : '';
    const markerEnd = document.kind === 'mindmap' || edge.data?.endMarker === 'none' ? '' : ` marker-end="url(#${markerId})"`;
    const color = edge.data?.color ?? '#738099';
    const strokeWidth = edge.data?.strokeWidth ?? 2;
    const label = edge.label || edge.data?.relation;
    const fontSize = edge.data?.fontSize ?? 13;
    const edgeY = document.kind === 'sequence' ? y1 : (y1 + y2) / 2;
    const labelMarkup = label
      ? `<text x="${(x1 + x2) / 2}" y="${edgeY - 7}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="${fontSize}" font-weight="600" fill="#465267" paint-order="stroke" stroke="#F9FBFE" stroke-width="5" stroke-linejoin="round">${escapeXml(label)}</text>`
      : '';
    const path = edge.type === 'straight' || document.kind === 'sequence'
      ? `M ${x1} ${y1} L ${x2} ${document.kind === 'sequence' ? y1 : y2}`
      : `M ${x1} ${y1} C ${(x1 + x2) / 2} ${y1}, ${(x1 + x2) / 2} ${y2}, ${x2} ${y2}`;
    return `<g><path d="${path}" fill="none" stroke="${color}" stroke-width="${strokeWidth}"${dash}${markerStart}${markerEnd}/>${labelMarkup}</g>`;
  }).join('');
  const nodeMarkup = positions.map(({ node, x, y, width, height }) => {
    const fill = node.data.fillColor ?? '#F7F9FC';
    const borderStyle = node.data.borderStyle ?? 'solid';
    const stroke = borderStyle === 'none' ? 'none' : node.data.borderColor ?? node.data.color ?? '#4E7CC7';
    const strokeWidth = node.data.borderWidth ?? 1;
    const textColor = node.data.textColor ?? '#1D2430';
    const fontSize = node.data.fontSize ?? (node.data.shape === 'text' ? 16 : 14);
    const fontWeight = node.data.fontWeight ?? (node.data.shape === 'text' ? 500 : 650);
    const borderDash = borderStyle === 'dotted' ? ' stroke-dasharray="2 4" stroke-linecap="round"' : borderStyle === 'dashed' ? ' stroke-dasharray="8 6"' : '';
    if (node.data.shape === 'lifeline') {
      return `<g><line x1="${x + width / 2}" y1="${y + 60}" x2="${x + width / 2}" y2="${y + height}" stroke="${stroke}" stroke-width="1.5" stroke-dasharray="7 6"/><rect x="${x}" y="${y}" width="${width}" height="60" rx="7" fill="${fill}" stroke="${stroke}" stroke-width="${Math.max(1.5, strokeWidth)}"/><text x="${x + width / 2}" y="${y + 36}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="${fontSize}" font-weight="${fontWeight}" fill="${textColor}">${escapeXml(node.data.label)}</text></g>`;
    }
    if (node.data.shape === 'activation') {
      return `<rect x="${x}" y="${y}" width="${width}" height="${height}" rx="2" fill="${fill}" stroke="${stroke}" stroke-width="${Math.max(1.5, strokeWidth)}"/>`;
    }
    if (node.data.shape === 'fragment') {
      return `<g><rect x="${x}" y="${y}" width="${width}" height="${height}" fill="none" stroke="${stroke}" stroke-width="${Math.max(1.5, strokeWidth)}"${borderDash}/><path d="M ${x} ${y + 28} H ${x + 92} L ${x + 105} ${y} H ${x}" fill="${fill}" stroke="${stroke}" stroke-width="1"/><text x="${x + 9}" y="${y + 19}" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="13" font-weight="650" fill="${textColor}">${escapeXml(node.data.label)}</text></g>`;
    }
    if (node.data.shape === 'lane') {
      return `<g><rect x="${x}" y="${y}" width="${width}" height="${height}" rx="14" fill="${fill}" stroke="${stroke}" stroke-width="${strokeWidth}"${borderDash}/><text x="${x + 18}" y="${y + 30}" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="14" font-weight="600" fill="#445066">${escapeXml(node.data.label)}</text></g>`;
    }
    if (node.data.shape === 'container') {
      return `<g><rect x="${x}" y="${y}" width="${width}" height="${height}" rx="12" fill="${fill}" stroke="${stroke}" stroke-width="${Math.max(1.5, strokeWidth)}"${borderDash}/><rect x="${x + 12}" y="${y + 9}" width="${Math.min(width - 24, Math.max(90, node.data.label.length * 14 + 18))}" height="25" rx="6" fill="#F9FBFE"/><text x="${x + 20}" y="${y + 27}" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="13" font-weight="650" fill="${textColor}">${escapeXml(node.data.label)}</text></g>`;
    }
    if (node.data.shape === 'mindmap-root') {
      const rootFill = node.data.fillColor ?? node.data.color ?? '#5D6FCD';
      const rootText = node.data.textColor ?? '#FFFFFF';
      return `<g data-mindmap-node="root"><rect x="${x}" y="${y}" width="${width}" height="${height}" rx="18" fill="${rootFill}" stroke="${stroke}" stroke-width="${Math.max(1.5, strokeWidth)}"${borderDash}/><text x="${x + width / 2}" y="${y + height / 2 + fontSize * 0.35}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="${fontSize}" font-weight="${fontWeight}" fill="${rootText}">${escapeXml(node.data.label)}</text></g>`;
    }
    if (node.data.shape === 'mindmap-topic') {
      return `<g data-mindmap-node="topic"><line x1="${x}" y1="${y + height - 2}" x2="${x + width}" y2="${y + height - 2}" stroke="${stroke}" stroke-width="${Math.max(2, strokeWidth)}"${borderDash}/><text x="${x + width / 2}" y="${y + height / 2 + fontSize * 0.2}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="${fontSize}" font-weight="${fontWeight}" fill="${textColor}">${escapeXml(node.data.label)}</text></g>`;
    }
    if (node.data.shape === 'text') {
      return `<text x="${x + width / 2}" y="${y + height / 2 + fontSize * 0.35}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="${fontSize}" font-weight="${fontWeight}" fill="#1D2430">${escapeXml(node.data.label)}</text>`;
    }
    if (node.data.shape === 'diamond') {
      const points = `${x + width / 2},${y} ${x + width},${y + height / 2} ${x + width / 2},${y + height} ${x},${y + height / 2}`;
      const label = node.data.showLabel === false ? '' : `<text x="${x + width / 2}" y="${y + height / 2 + fontSize * 0.35}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="${fontSize}" font-weight="${fontWeight}" fill="${textColor}">${escapeXml(node.data.label)}</text>`;
      return `<g><polygon points="${points}" fill="${fill}" stroke="${stroke}" stroke-width="${strokeWidth}"${borderDash}/>${label}</g>`;
    }
    const radius = node.data.shape === 'circle' ? Math.min(width, height) / 2 : 14;
    const label = node.data.showLabel === false ? '' : `<text x="${x + width / 2}" y="${y + height / 2 + (node.data.subtitle ? -1 : fontSize * 0.35)}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="${fontSize}" font-weight="${fontWeight}" fill="${textColor}">${escapeXml(node.data.label)}</text>${node.data.subtitle ? `<text x="${x + width / 2}" y="${y + height / 2 + 18}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="11.5" fill="${textColor}" opacity=".72">${escapeXml(node.data.subtitle)}</text>` : ''}`;
    return `<g><rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${radius}" fill="${fill}" stroke="${stroke}" stroke-width="${strokeWidth}"${borderDash}/>${label}</g>`;
  }).join('');
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${minX} ${minY} ${maxX - minX} ${maxY - minY}" width="${maxX - minX}" height="${maxY - minY}"><defs><marker id="arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto-start-reverse"><path d="M0,0 L8,4 L0,8 z" fill="context-stroke"/></marker><marker id="sequence-call-arrow" markerWidth="7" markerHeight="7" refX="6.5" refY="3.5" orient="auto-start-reverse"><path d="M0,0 L7,3.5 L0,7 z" fill="context-stroke"/></marker><marker id="sequence-return-arrow" markerWidth="7" markerHeight="7" refX="6.5" refY="3.5" orient="auto-start-reverse"><path d="M0.5,0.5 L6.5,3.5 L0.5,6.5" fill="none" stroke="context-stroke" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/></marker></defs><rect x="${minX}" y="${minY}" width="${maxX - minX}" height="${maxY - minY}" fill="#F9FBFE"/>${edgeMarkup}${nodeMarkup}</svg>`;
}

function svgHandlePoint(
  item: { node: DiagramNode; x: number; y: number; width: number; height: number },
  handleId?: string
): { x: number; y: number } {
  if (item.node.data.shape === 'lifeline') {
    const slot = parseSequenceSlot(handleId);
    return { x: item.x + item.width / 2, y: item.y + item.height * (slot === undefined ? 50 : sequenceSlotPercentage(slot)) / 100 };
  }
  if (item.node.data.shape === 'activation') {
    const handle = parseSequenceActivationHandle(handleId);
    if (handle) {
      return {
        x: handle.side === 'left' ? item.x : item.x + item.width,
        y: item.y + item.height * sequenceActivationSlotPercentage(handle.slot, handle.version) / 100
      };
    }
  }
  switch (handleId) {
    case 'left': return { x: item.x, y: item.y + item.height / 2 };
    case 'right': return { x: item.x + item.width, y: item.y + item.height / 2 };
    case 'top': return { x: item.x + item.width / 2, y: item.y };
    case 'bottom': return { x: item.x + item.width / 2, y: item.y + item.height };
    default: return { x: item.x + item.width / 2, y: item.y + item.height / 2 };
  }
}
