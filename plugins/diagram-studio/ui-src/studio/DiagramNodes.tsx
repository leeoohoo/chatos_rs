import { Handle, NodeResizeControl, NodeResizer, Position, type NodeProps } from '@xyflow/react';
import type { DiagramNode, DiagramNodeData, DiagramNodeShape } from '../../src/schema';
import {
  legacySequenceActivationSlotCount,
  sequenceActivationHandleId,
  sequenceActivationSlotCount,
  sequenceActivationSlotPercentage,
  sequenceLifelineSlotCount,
  sequenceSlotPercentage
} from '../../src/sequence';
import { Icon } from './Icons';

function NodeHandles() {
  return <>
    <Handle type="source" position={Position.Left} id="left" />
    <Handle type="source" position={Position.Right} id="right" />
    <Handle type="source" position={Position.Top} id="top" />
    <Handle type="source" position={Position.Bottom} id="bottom" />
  </>;
}

export function DiagramNodeView({ data, selected }: NodeProps<DiagramNode>) {
  const style = {
    '--node-accent': data.color ?? '#4E7CC7',
    '--node-default-fill': 'color-mix(in srgb, var(--surface-solid) 88%, var(--node-accent))',
    '--node-fill': data.fillColor,
    '--node-border-color': data.borderColor ?? data.color ?? '#4E7CC7',
    '--node-border-style': data.borderStyle ?? 'solid',
    '--node-border-width': `${data.borderWidth ?? 1}px`,
    '--node-font-size': `${data.fontSize ?? (data.shape === 'text' ? 16 : 14)}px`,
    '--node-font-weight': data.fontWeight ?? (data.shape === 'text' ? 500 : 650)
  } as React.CSSProperties;
  const resizer = data.shape === 'activation'
    ? Boolean(selected) ? <>
      <NodeResizeControl position="top" resizeDirection="vertical" minWidth={14} maxWidth={14} minHeight={44} className="activation-height-handle" />
      <NodeResizeControl position="bottom" resizeDirection="vertical" minWidth={14} maxWidth={14} minHeight={44} className="activation-height-handle" />
    </> : null
    : <NodeResizer
    isVisible={Boolean(selected)}
    minWidth={data.shape === 'lifeline' ? 120 : data.shape === 'fragment' ? 220 : data.shape === 'text' ? 60 : 44}
    minHeight={data.shape === 'lifeline' ? 260 : data.shape === 'fragment' ? 100 : data.shape === 'text' ? 28 : 44}
    keepAspectRatio={data.shape === 'circle'}
    lineClassName="node-resizer-line"
    handleClassName="node-resizer-handle"
  />;

  if (data.shape === 'text') {
    return (
      <div className={`diagram-text-node ${selected ? 'selected' : ''}`} style={style}>
        {resizer}
        <span>{data.label || '文本'}</span>
      </div>
    );
  }

  if (data.shape === 'lifeline') {
    return (
      <div className={`sequence-lifeline-node ${selected ? 'selected' : ''}`} style={style}>
        {resizer}
        <div className="sequence-participant">
          {data.icon && <span><Icon name={data.icon} /></span>}
          <strong>{data.label}</strong>
        </div>
        <div className="sequence-lifeline-stem" />
        {Array.from({ length: sequenceLifelineSlotCount }, (_, index) => <Handle key={`slot-${index}`} type="source" position={Position.Left} id={`slot-${index}`} style={{ left: '50%', top: `${sequenceSlotPercentage(index)}%` }} />)}
      </div>
    );
  }

  if (data.shape === 'activation') {
    return <div className={`sequence-activation-node ${selected ? 'selected' : ''}`} style={style}>
      {resizer}
      <div className="activation-drag-handle" title="拖动激活条" aria-hidden="true" />
      {([1, 2] as const).flatMap((version) => (['left', 'right'] as const).flatMap((side) => Array.from({
        length: version === 1 ? legacySequenceActivationSlotCount : sequenceActivationSlotCount
      }, (_, index) => (
          <Handle
            key={sequenceActivationHandleId(side, index, version)}
            type="source"
            position={side === 'left' ? Position.Left : Position.Right}
            id={sequenceActivationHandleId(side, index, version)}
            style={{ top: `${sequenceActivationSlotPercentage(index, version)}%` }}
          />
        ))))}
    </div>;
  }

  if (data.shape === 'fragment') {
    return (
      <div className={`sequence-fragment-node ${selected ? 'selected' : ''}`} style={style}>
        {resizer}
        <span>{data.label || 'alt'}</span>
      </div>
    );
  }

  if (data.icon && data.showLabel === false) {
    return (
      <div className={`diagram-icon-node ${selected ? 'selected' : ''}`} style={style}>
        {resizer}
        <NodeHandles />
        <NodeSurface data={data} shape="rounded" />
        <Icon name={data.icon} />
      </div>
    );
  }

  if (data.shape === 'diamond') {
    return (
      <div className={`diagram-node diamond ${selected ? 'selected' : ''}`} style={style}>
        {resizer}
        <NodeHandles />
        <NodeSurface data={data} shape="diamond" />
        <div className="diamond-surface"><div className="diamond-content">{data.showLabel !== false && <strong>{data.label}</strong>}</div></div>
      </div>
    );
  }
  return (
    <div className={`diagram-node shape-${data.shape} category-${data.category} ${selected ? 'selected' : ''}`} style={style}>
      {resizer}
      <NodeHandles />
      <NodeSurface data={data} shape={data.shape} />
      {data.icon
        ? <span className="node-symbol node-icon"><Icon name={data.icon} /></span>
        : data.showLabel !== false && <span className="node-symbol" />}
      {data.showLabel !== false && <div className="node-copy">
          <strong>{data.label}</strong>
          {data.subtitle && <span>{data.subtitle}</span>}
        </div>}
    </div>
  );
}

function NodeSurface({ data, shape }: { data: DiagramNodeData; shape: DiagramNodeShape }) {
  const borderStyle = data.borderStyle ?? 'solid';
  const strokeDasharray = borderStyle === 'dashed' ? '10 7' : borderStyle === 'dotted' ? '1 7' : undefined;
  const common = {
    className: 'node-surface-path',
    fill: data.fillColor ?? 'transparent',
    stroke: borderStyle === 'none' ? 'none' : data.borderColor ?? data.color ?? '#4E7CC7',
    strokeWidth: Math.max(1.5, data.borderWidth ?? 2),
    strokeDasharray,
    strokeLinecap: borderStyle === 'dotted' ? 'round' as const : 'butt' as const,
    vectorEffect: 'non-scaling-stroke' as const
  };
  return <svg className="node-surface-svg" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
    {shape === 'diamond'
      ? <polygon points="50,3 97,50 50,97 3,50" {...common} />
      : shape === 'circle'
        ? <ellipse cx="50" cy="50" rx="48" ry="48" {...common} />
        : shape === 'cylinder'
          ? <rect x="2" y="2" width="96" height="96" rx="48" ry="18" {...common} />
          : <rect x="2" y="2" width="96" height="96" rx={shape === 'rectangle' ? 2 : 11} ry={shape === 'rectangle' ? 2 : 11} {...common} />}
  </svg>;
}

export function LaneNodeView({ data, selected }: NodeProps<DiagramNode>) {
  return (
    <div className={`lane-node ${selected ? 'selected' : ''}`} style={{ background: data.fillColor ?? data.color ?? '#EEF2F8' }}>
      <div className="lane-title">{data.label}</div>
    </div>
  );
}
