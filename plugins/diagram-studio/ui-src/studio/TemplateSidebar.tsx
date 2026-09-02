import { useMemo, useState } from 'react';
import type { DiagramKind, DiagramNodeCategory, DiagramNodeIcon, DiagramNodeShape } from '../../src/schema';
import { Icon } from './Icons';

export interface PaletteItem {
  id: string;
  label: string;
  category: DiagramNodeCategory;
  shape: DiagramNodeShape;
  color: string;
  icon?: DiagramNodeIcon;
  width?: number;
  height?: number;
  showLabel?: boolean;
  fillColor?: string;
  borderStyle?: 'solid' | 'dashed' | 'dotted' | 'none';
}

export type SequenceMessagePreset = 'call' | 'return';

export const componentDragType = 'application/x-chatos-diagram-component';

interface PaletteSection {
  title: string;
  items: PaletteItem[];
}

const textItem: PaletteItem = { id: 'text', label: '文本', category: 'note', shape: 'text', color: '#667085', showLabel: true };
const basicShapes: PaletteItem[] = [
  { id: 'rounded', label: '圆角矩形', category: 'process', shape: 'rounded', color: '#4E7CC7' },
  { id: 'rectangle', label: '矩形', category: 'process', shape: 'rectangle', color: '#667085' },
  { id: 'circle', label: '圆形', category: 'terminal', shape: 'circle', color: '#7967D8' },
  { id: 'diamond', label: '菱形', category: 'decision', shape: 'diamond', color: '#C98145' },
  textItem
];

const paletteByKind: Record<DiagramKind, PaletteSection[]> = {
  architecture: [
    { title: '基础图形', items: basicShapes },
    { title: '用户与终端', items: [
      { id: 'user', label: '用户', category: 'client', shape: 'rounded', icon: 'user', color: '#7967D8' },
      { id: 'terminal', label: '桌面终端', category: 'client', shape: 'rounded', icon: 'terminal', color: '#4E7CC7' },
      { id: 'mobile', label: '移动终端', category: 'client', shape: 'rounded', icon: 'mobile', color: '#4E7CC7' },
      { id: 'browser', label: '浏览器', category: 'client', shape: 'rounded', icon: 'browser', color: '#438FA6' }
    ] },
    { title: '服务与数据', items: [
      { id: 'service', label: '应用服务', category: 'service', shape: 'rounded', icon: 'server', color: '#7967D8' },
      { id: 'api', label: 'API 网关', category: 'network', shape: 'rounded', icon: 'api', color: '#438FA6' },
      { id: 'database', label: '数据库', category: 'database', shape: 'rounded', icon: 'database', color: '#4B9B72' },
      { id: 'cache', label: '缓存', category: 'database', shape: 'rounded', icon: 'cache', color: '#B9658D' },
      { id: 'storage', label: '对象存储', category: 'database', shape: 'rounded', icon: 'storage', color: '#4B9B72' },
      { id: 'queue', label: '消息队列', category: 'queue', shape: 'rounded', icon: 'queue', color: '#C98145' }
    ] },
    { title: '运行与边界', items: [
      { id: 'cloud', label: '云服务', category: 'external', shape: 'rounded', icon: 'cloud', color: '#4E7CC7' },
      { id: 'container', label: '容器', category: 'service', shape: 'rounded', icon: 'container', color: '#4E7CC7' },
      { id: 'cluster', label: '集群', category: 'network', shape: 'rounded', icon: 'cluster', color: '#7967D8' },
      { id: 'shield', label: '安全服务', category: 'network', shape: 'rounded', icon: 'shield', color: '#B9658D' },
      { id: 'external', label: '外部系统', category: 'external', shape: 'rounded', icon: 'network', color: '#667085' },
      { id: 'monitor', label: '监控', category: 'external', shape: 'rounded', icon: 'monitor', color: '#4B9B72' }
    ] }
  ],
  flowchart: [
    { title: '流程图形', items: [
      { id: 'flow-start', label: '开始 / 结束', category: 'terminal', shape: 'rounded', color: '#4B9B72' },
      { id: 'flow-process', label: '处理步骤', category: 'process', shape: 'rectangle', color: '#4E7CC7' },
      { id: 'flow-decision', label: '条件判断', category: 'decision', shape: 'diamond', color: '#C98145' },
      { id: 'flow-io', label: '输入 / 输出', category: 'process', shape: 'rounded', color: '#438FA6' },
      { id: 'flow-database', label: '数据存储', category: 'database', shape: 'cylinder', color: '#4B9B72' },
      { id: 'flow-connector', label: '页内连接', category: 'process', shape: 'circle', color: '#7967D8' },
      { id: 'flow-document', label: '文档', category: 'note', shape: 'rounded', icon: 'document', color: '#667085' },
      textItem
    ] },
    { title: '说明', items: [
      { id: 'flow-note', label: '备注', category: 'note', shape: 'rounded', icon: 'note', color: '#8D96A6' }
    ] }
  ],
  swimlane: [
    { title: '泳道结构', items: [
      { id: 'swimlane', label: '水平泳道', category: 'lane', shape: 'lane', color: '#EEF4FC', width: 900, height: 180, showLabel: true },
      { id: 'swimlane-tall', label: '垂直泳道', category: 'lane', shape: 'lane', color: '#F3F0FB', width: 260, height: 620, showLabel: true }
    ] },
    { title: '泳道内流程', items: [
      { id: 'lane-start', label: '开始 / 结束', category: 'terminal', shape: 'rounded', color: '#4B9B72' },
      { id: 'lane-process', label: '处理步骤', category: 'process', shape: 'rectangle', color: '#4E7CC7' },
      { id: 'lane-decision', label: '条件判断', category: 'decision', shape: 'diamond', color: '#C98145' },
      { id: 'lane-manual', label: '人工处理', category: 'process', shape: 'rounded', icon: 'user', color: '#7967D8' },
      { id: 'lane-document', label: '交付文档', category: 'note', shape: 'rounded', icon: 'document', color: '#667085' },
      textItem
    ] }
  ],
  topology: [
    { title: '网络与边界', items: [
      { id: 'topo-cloud', label: 'Internet / 云', category: 'external', shape: 'rounded', icon: 'cloud', color: '#4E7CC7' },
      { id: 'topo-network', label: '网络节点', category: 'network', shape: 'rounded', icon: 'network', color: '#438FA6' },
      { id: 'topo-firewall', label: '防火墙', category: 'network', shape: 'rounded', icon: 'shield', color: '#B9658D' },
      { id: 'topo-gateway', label: '网关 / 负载均衡', category: 'network', shape: 'rounded', icon: 'api', color: '#438FA6' }
    ] },
    { title: '计算节点', items: [
      { id: 'topo-server', label: '服务器', category: 'network', shape: 'rounded', icon: 'server', color: '#667085' },
      { id: 'topo-container', label: '容器', category: 'service', shape: 'rounded', icon: 'container', color: '#4E7CC7' },
      { id: 'topo-cluster', label: '集群', category: 'network', shape: 'rounded', icon: 'cluster', color: '#7967D8' },
      { id: 'topo-terminal', label: '终端', category: 'client', shape: 'rounded', icon: 'terminal', color: '#4E7CC7' }
    ] },
    { title: '数据与观测', items: [
      { id: 'topo-database', label: '数据库', category: 'database', shape: 'rounded', icon: 'database', color: '#4B9B72' },
      { id: 'topo-storage', label: '存储', category: 'database', shape: 'rounded', icon: 'storage', color: '#4B9B72' },
      { id: 'topo-cache', label: '缓存', category: 'database', shape: 'rounded', icon: 'cache', color: '#B9658D' },
      { id: 'topo-monitor', label: '监控', category: 'external', shape: 'rounded', icon: 'monitor', color: '#4B9B72' },
      textItem
    ] }
  ],
  sequence: [
    { title: '参与者与生命线', items: [
      { id: 'seq-participant', label: '参与者生命线', category: 'service', shape: 'lifeline', color: '#4E7CC7', width: 160, height: 560, showLabel: true }
    ] },
    { title: '执行与结构', items: [
      { id: 'seq-activation', label: '激活条', category: 'process', shape: 'activation', color: '#4E7CC7', width: 14, height: 120 },
      { id: 'seq-fragment', label: 'alt / opt / loop', category: 'note', shape: 'fragment', color: '#667085', width: 620, height: 220, showLabel: true },
      { id: 'seq-note', label: '注释', category: 'note', shape: 'rounded', icon: 'note', color: '#8D96A6' },
      textItem
    ] }
  ]
};

export function TemplateSidebar({
  diagramKind,
  onAddNode,
  sequenceMessagePreset,
  onSequenceMessagePresetChange
}: {
  diagramKind: DiagramKind;
  onAddNode: (item: PaletteItem) => void;
  sequenceMessagePreset: SequenceMessagePreset;
  onSequenceMessagePresetChange: (preset: SequenceMessagePreset) => void;
}) {
  const [query, setQuery] = useState('');
  const sections = paletteByKind[diagramKind];
  const itemCount = sections.reduce((total, section) => total + section.items.length, 0);
  const visibleSections = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return sections;
    return sections
      .map((section) => ({
        ...section,
        items: section.items.filter((item) => `${item.label} ${section.title}`.toLocaleLowerCase().includes(normalized))
      }))
      .filter((section) => section.items.length > 0);
  }, [query, sections]);

  return (
    <aside className="studio-sidebar">
      <div className="sidebar-header">
        <strong>{kindName(diagramKind)}组件</strong>
        <span>拖到画布 · {itemCount} 个</span>
      </div>
      <label className="component-search">
        <Icon name="search" />
        <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索组件" aria-label="搜索组件" />
        {query && <button onClick={() => setQuery('')} aria-label="清空搜索"><Icon name="close" /></button>}
      </label>
      <div className="sidebar-scroll">
        {diagramKind === 'sequence' && <section>
          <div className="section-label">消息连线</div>
          <div className="sequence-message-presets" role="group" aria-label="时序图消息类型">
            <button className={sequenceMessagePreset === 'call' ? 'active' : ''} onClick={() => onSequenceMessagePresetChange('call')}><i className="message-line solid" /><span>同步调用</span></button>
            <button className={sequenceMessagePreset === 'return' ? 'active' : ''} onClick={() => onSequenceMessagePresetChange('return')}><i className="message-line dashed" /><span>返回消息</span></button>
          </div>
          <p className="sequence-help">选择消息类型，可从生命线任意高度拖出连线。参与者名称和图标可在检查器修改。</p>
        </section>}
        {visibleSections.map((section) => (
          <section key={section.title}>
            <div className="section-label">{section.title}</div>
            <div className="palette-grid">
              {section.items.map((item) => (
                <button
                  key={item.id}
                  className="palette-item"
                  draggable
                  onDragStart={(event) => {
                    event.dataTransfer.effectAllowed = 'copy';
                    event.dataTransfer.setData(componentDragType, JSON.stringify(item));
                    event.dataTransfer.setData('text/plain', item.label);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ' ') {
                      event.preventDefault();
                      onAddNode(item);
                    }
                  }}
                  title={`拖动“${item.label}”到画布`}
                  aria-label={`${item.label}，拖动到画布`}
                >
                  <span className={`palette-symbol ${item.icon && item.shape !== 'lifeline' ? 'is-icon' : ''}`} style={{ '--item-color': item.color } as React.CSSProperties}>
                    {item.shape === 'lifeline'
                      ? <span className={`palette-lifeline ${item.icon ? 'has-icon' : ''}`}>{item.icon && <Icon name={item.icon} />}<i /></span>
                      : item.shape === 'activation'
                        ? <span className="palette-activation" />
                        : item.shape === 'fragment'
                          ? <span className="palette-fragment"><i>alt</i></span>
                          : item.icon
                            ? <Icon name={item.icon} />
                            : item.shape === 'text'
                              ? <span className="palette-text-symbol">T</span>
                              : item.shape === 'lane'
                                ? <span className="palette-lane" />
                                : <span className={`palette-shape shape-${item.shape}`} />}
                  </span>
                  <span>{item.label}</span>
                </button>
              ))}
            </div>
          </section>
        ))}
        {visibleSections.length === 0 && <div className="empty-components"><Icon name="search" /><span>没有匹配的组件</span></div>}
      </div>
    </aside>
  );
}

function kindName(kind: DiagramKind): string {
  switch (kind) {
    case 'architecture': return '架构图';
    case 'flowchart': return '流程图';
    case 'swimlane': return '泳道图';
    case 'topology': return '拓扑图';
    case 'sequence': return '时序图';
  }
}
