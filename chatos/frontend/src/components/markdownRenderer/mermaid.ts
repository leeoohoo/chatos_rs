// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface MermaidNormalizationResult {
  code: string;
  changed: boolean;
  notes: string[];
}

export interface MermaidExportNotice {
  type: 'success' | 'error';
  text: string;
}

export type MermaidPreviewStatus = 'idle' | 'loading' | 'rendered' | 'error';

export type MermaidApi = {
  initialize: (config: {
    startOnLoad: boolean;
    securityLevel: 'strict' | 'loose' | 'antiscript' | 'sandbox';
    theme: 'default' | 'dark';
    suppressErrorRendering: boolean;
  }) => void;
  parse: (text: string, parseOptions?: { suppressErrors?: boolean }) => Promise<unknown>;
  render: (
    id: string,
    text: string,
  ) => Promise<{ svg: string; bindFunctions?: (element: Element) => void }>;
};

const normalizeFlowchartMermaid = (sourceCode: string): MermaidNormalizationResult => {
  let normalizedCode = sourceCode;
  const notes: string[] = [];
  const isFlowchart = /^\s*(flowchart|graph)\b/im.test(sourceCode);

  if (!isFlowchart) {
    return { code: sourceCode, changed: false, notes };
  }

  if (normalizedCode.includes('-->>')) {
    normalizedCode = normalizedCode.replace(/-->>/g, '-->');
    notes.push('flowchart edge "-->>" normalized to "-->"');
  }

  const edgeWithColonPattern = /^(\s*)(.+?)\s*-->\s*(.+?)\s*:\s*(.+?)\s*$/;
  const rewrittenLines = normalizedCode.split('\n').map((line) => {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('%%')) {
      return line;
    }

    const match = line.match(edgeWithColonPattern);
    if (!match) {
      return line;
    }

    const [, indent, from, to, label] = match;
    const safeLabel = label.trim().replace(/\|/g, '\\|');
    return `${indent}${from.trim()} -->|${safeLabel}| ${to.trim()}`;
  });
  const rewrittenCode = rewrittenLines.join('\n');
  if (rewrittenCode !== normalizedCode) {
    notes.push('flowchart edge labels with ":" normalized to "|label|"');
    normalizedCode = rewrittenCode;
  }

  return {
    code: normalizedCode,
    changed: normalizedCode !== sourceCode,
    notes,
  };
};

const normalizeSequenceMermaid = (sourceCode: string): MermaidNormalizationResult => {
  const notes: string[] = [];
  const isSequenceDiagram = /^\s*sequenceDiagram\b/im.test(sourceCode);
  if (!isSequenceDiagram) {
    return { code: sourceCode, changed: false, notes };
  }

  const blockStartPattern = /^\s*(alt|opt|loop|par|critical|break|rect)\b/i;
  const lines = sourceCode.split('\n');
  const normalizedLines: string[] = [];
  let openBlocks = 0;
  let removedEndCount = 0;

  lines.forEach((line) => {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('%%')) {
      normalizedLines.push(line);
      return;
    }
    if (blockStartPattern.test(trimmed)) {
      openBlocks += 1;
      normalizedLines.push(line);
      return;
    }
    if (/^end$/i.test(trimmed)) {
      if (openBlocks > 0) {
        openBlocks -= 1;
        normalizedLines.push(line);
      } else {
        removedEndCount += 1;
      }
      return;
    }
    normalizedLines.push(line);
  });

  if (removedEndCount <= 0) {
    return { code: sourceCode, changed: false, notes };
  }

  notes.push(`sequenceDiagram removed ${removedEndCount} unmatched "end" lines`);
  return {
    code: normalizedLines.join('\n'),
    changed: true,
    notes,
  };
};

export const normalizeMermaidForRetry = (sourceCode: string): MermaidNormalizationResult => {
  let currentCode = sourceCode;
  let changed = false;
  const notes: string[] = [];

  const flowchartNormalized = normalizeFlowchartMermaid(currentCode);
  if (flowchartNormalized.changed) {
    currentCode = flowchartNormalized.code;
    changed = true;
    notes.push(...flowchartNormalized.notes);
  }

  const sequenceNormalized = normalizeSequenceMermaid(currentCode);
  if (sequenceNormalized.changed) {
    currentCode = sequenceNormalized.code;
    changed = true;
    notes.push(...sequenceNormalized.notes);
  }

  return {
    code: currentCode,
    changed,
    notes,
  };
};

const resolveSvgRenderSize = (
  svgElement: SVGSVGElement,
): { width: number; height: number } | null => {
  let width = Number.parseFloat(svgElement.getAttribute('width') || '');
  let height = Number.parseFloat(svgElement.getAttribute('height') || '');

  if (!(width > 0) || !(height > 0)) {
    const viewBox = svgElement.getAttribute('viewBox');
    if (viewBox) {
      const values = viewBox
        .split(/[\s,]+/)
        .map((item) => Number(item))
        .filter((item) => Number.isFinite(item));
      if (values.length === 4) {
        width = values[2];
        height = values[3];
      }
    }
  }

  if (!(width > 0) || !(height > 0)) {
    const rect = svgElement.getBoundingClientRect();
    width = rect.width;
    height = rect.height;
  }

  if (!(width > 0) || !(height > 0)) {
    return null;
  }

  return { width, height };
};

export const createMermaidSvgSnapshot = (
  svgElement: SVGSVGElement,
): { svgText: string; width: number; height: number } => {
  const size = resolveSvgRenderSize(svgElement);
  if (!size) {
    throw new Error('Cannot resolve Mermaid svg size');
  }

  const exportSvg = svgElement.cloneNode(true) as SVGSVGElement;
  const width = Math.max(1, Math.ceil(size.width));
  const height = Math.max(1, Math.ceil(size.height));
  exportSvg.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  exportSvg.setAttribute('xmlns:xlink', 'http://www.w3.org/1999/xlink');
  exportSvg.setAttribute('width', `${width}`);
  exportSvg.setAttribute('height', `${height}`);
  if (!exportSvg.getAttribute('viewBox')) {
    exportSvg.setAttribute('viewBox', `0 0 ${width} ${height}`);
  }

  const serializer = new XMLSerializer();
  const svgText = serializer.serializeToString(exportSvg);
  return { svgText, width, height };
};
