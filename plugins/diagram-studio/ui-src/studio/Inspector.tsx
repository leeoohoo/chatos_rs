import type { DiagramEdge, DiagramNode, DiagramNodeCategory, DiagramNodeIcon, DiagramNodeShape } from '../../src/schema';
import { Icon } from './Icons';

const strokeColors = ['#1D2430', '#4E7CC7', '#7967D8', '#4B9B72', '#C98145', '#B9658D', '#438FA6', '#667085'];
const fillColors = ['#FFFFFF', '#E8F1FF', '#EEEAFE', '#E8F6ED', '#FFF0E4', '#FBEAF2', '#E8F5F7', '#EEF1F5'];

export function Inspector({
  node,
  edge,
  onUpdateNode,
  onUpdateEdge,
  onDelete,
  onClose
}: {
  node?: DiagramNode;
  edge?: DiagramEdge;
  onUpdateNode: (node: DiagramNode) => void;
  onUpdateEdge: (edge: DiagramEdge) => void;
  onDelete: () => void;
  onClose: () => void;
}) {
  return (
    <aside className="studio-inspector">
      <header className="inspector-header">
        <strong>{node ? '节点检查器' : edge ? '连线检查器' : '检查器'}</strong>
        <button className="icon-button subtle" onClick={onClose} aria-label="关闭检查器"><Icon name="close" /></button>
      </header>
      {!node && !edge && (
        <div className="empty-inspector">
          <div className="empty-glyph"><Icon name="inspector" /></div>
          <strong>选择一个对象</strong>
          <p>选中节点或连线后，可以在这里修改名称、类型、颜色和说明。</p>
        </div>
      )}
      {node && (
        <div className="inspector-form">
          <FormSection title="内容">
            <label>名称<input value={node.data.label} onChange={(event) => onUpdateNode({ ...node, data: { ...node.data, label: event.target.value } })} /></label>
            <label>副标题<input value={node.data.subtitle ?? ''} onChange={(event) => onUpdateNode({ ...node, data: { ...node.data, subtitle: event.target.value } })} /></label>
            <label>说明<textarea rows={4} value={node.data.description ?? ''} onChange={(event) => onUpdateNode({ ...node, data: { ...node.data, description: event.target.value } })} /></label>
          </FormSection>
          <FormSection title="样式">
            {node.data.shape !== 'lifeline' && <label>类别<select value={node.data.category} onChange={(event) => onUpdateNode({ ...node, data: { ...node.data, category: event.target.value as DiagramNodeCategory } })}>
              <option value="client">客户端</option><option value="service">服务</option><option value="database">数据库</option>
              <option value="queue">消息队列</option><option value="network">网络节点</option><option value="external">外部系统</option>
              <option value="process">流程步骤</option><option value="decision">判断</option><option value="terminal">开始 / 结束</option><option value="note">说明 / 文档</option>
              <option value="lane">泳道 / 参与者</option>
            </select></label>}
            <label>形状<select value={node.data.shape} onChange={(event) => onUpdateNode({ ...node, data: { ...node.data, shape: event.target.value as DiagramNodeShape } })}>
              <option value="rounded">圆角矩形</option><option value="rectangle">矩形</option><option value="circle">圆形</option>
              <option value="diamond">菱形</option><option value="cylinder">数据库</option><option value="text">纯文本</option>
              <option value="lane">泳道</option><option value="lifeline">参与者生命线</option><option value="activation">激活条</option><option value="fragment">组合片段</option>
            </select></label>
            <label>图标<select value={node.data.icon ?? ''} onChange={(event) => onUpdateNode({ ...node, data: { ...node.data, icon: (event.target.value || undefined) as DiagramNodeIcon | undefined } })}>
              <option value="">无图标</option><option value="user">用户</option><option value="terminal">桌面终端</option><option value="mobile">移动终端</option>
              <option value="browser">浏览器</option><option value="server">服务器</option><option value="api">API</option><option value="cloud">云服务</option>
              <option value="database">数据库</option><option value="cache">缓存</option><option value="storage">对象存储</option><option value="queue">消息队列</option>
              <option value="network">网络</option><option value="shield">安全</option><option value="container">容器</option><option value="cluster">集群</option>
              <option value="monitor">监控</option><option value="document">文档</option><option value="note">备注</option>
            </select></label>
            {(node.data.shape === 'text' || node.data.showLabel !== false) && <label>字号<select value={String(node.data.fontSize ?? (node.data.shape === 'text' ? 16 : 14))} onChange={(event) => onUpdateNode({ ...node, data: { ...node.data, fontSize: Number(event.target.value) } })}>
              <option value="12">12</option><option value="14">14</option><option value="16">16</option><option value="20">20</option><option value="24">24</option><option value="32">32</option>
            </select></label>}
            {node.data.shape !== 'text' && <label className="checkbox-row"><input type="checkbox" checked={node.data.showLabel !== false} onChange={(event) => onUpdateNode({ ...node, data: { ...node.data, showLabel: event.target.checked } })} />在图形内显示名称</label>}
            {node.data.shape !== 'text' && <LineStylePicker label="边框样式" value={node.data.borderStyle ?? 'solid'} includeNone onChange={(borderStyle) => onUpdateNode({ ...node, data: { ...node.data, borderStyle } })} />}
            {node.data.shape !== 'text' && <label>边框粗细<select value={String(node.data.borderWidth ?? 1)} onChange={(event) => onUpdateNode({ ...node, data: { ...node.data, borderWidth: Number(event.target.value) } })}>
              <option value="1">细</option><option value="1.5">标准</option><option value="2.5">粗</option><option value="4">很粗</option>
            </select></label>}
            {node.data.shape !== 'text' && <ColorPicker label="填充颜色" value={node.data.fillColor} colors={fillColors} onChange={(fillColor) => onUpdateNode({ ...node, data: { ...node.data, fillColor } })} />}
            {node.data.shape !== 'text' && <ColorPicker label="边框颜色" value={node.data.borderColor ?? node.data.color} colors={strokeColors} onChange={(borderColor) => onUpdateNode({ ...node, data: { ...node.data, borderColor } })} />}
            {node.data.icon && <ColorPicker label="图标颜色" value={node.data.color} colors={strokeColors} onChange={(color) => onUpdateNode({ ...node, data: { ...node.data, color } })} />}
          </FormSection>
          <FormSection title="源码依据">
            <label>路径或说明<textarea rows={4} placeholder="例如 services/api/src/main.rs:42" value={(node.data.sourceReferences ?? []).join('\n')} onChange={(event) => onUpdateNode({ ...node, data: { ...node.data, sourceReferences: event.target.value.split('\n').map((value) => value.trim()).filter(Boolean) } })} /></label>
          </FormSection>
        </div>
      )}
      {edge && (
        <div className="inspector-form">
          <FormSection title="连线">
            <label>标签<input value={edge.label ?? ''} onChange={(event) => onUpdateEdge({ ...edge, label: event.target.value, data: { ...edge.data, relation: event.target.value } })} /></label>
            <label>说明<textarea rows={4} value={edge.data?.description ?? ''} onChange={(event) => onUpdateEdge({ ...edge, data: { ...edge.data, description: event.target.value } })} /></label>
          </FormSection>
          <FormSection title="线条">
            <LineStylePicker label="线型" value={edge.data?.lineStyle ?? (edge.data?.dashed ? 'dashed' : 'solid')} onChange={(lineStyle) => onUpdateEdge({ ...edge, data: { ...edge.data, dashed: undefined, lineStyle: lineStyle === 'none' ? 'solid' : lineStyle } })} />
            <label>路径<select value={edge.type ?? 'smoothstep'} onChange={(event) => onUpdateEdge({ ...edge, type: event.target.value as DiagramEdge['type'] })}>
              <option value="smoothstep">折线</option><option value="straight">直线</option><option value="bezier">曲线</option>
            </select></label>
            <label>起点<select value={edge.data?.startMarker ?? 'none'} onChange={(event) => onUpdateEdge({ ...edge, data: { ...edge.data, startMarker: event.target.value as 'none' | 'arrow' } })}>
              <option value="none">无</option><option value="arrow">箭头</option>
            </select></label>
            <label>终点<select value={edge.data?.endMarker ?? 'arrow'} onChange={(event) => onUpdateEdge({ ...edge, data: { ...edge.data, endMarker: event.target.value as 'none' | 'arrow' } })}>
              <option value="arrow">箭头</option><option value="none">无</option>
            </select></label>
            <label>线宽<select value={String(edge.data?.strokeWidth ?? 1.7)} onChange={(event) => onUpdateEdge({ ...edge, data: { ...edge.data, strokeWidth: Number(event.target.value) } })}>
              <option value="1">细</option><option value="1.7">标准</option><option value="2.5">粗</option><option value="4">很粗</option>
            </select></label>
            <label>标签字号<select value={String(edge.data?.fontSize ?? 13)} onChange={(event) => onUpdateEdge({ ...edge, data: { ...edge.data, fontSize: Number(event.target.value) } })}>
              <option value="11">11</option><option value="13">13</option><option value="15">15</option><option value="18">18</option><option value="22">22</option>
            </select></label>
            <ColorPicker label="线条颜色" value={edge.data?.color} colors={strokeColors} onChange={(color) => onUpdateEdge({ ...edge, data: { ...edge.data, color } })} />
          </FormSection>
        </div>
      )}
      {(node || edge) && <div className="inspector-footer"><button className="destructive-button" onClick={onDelete}><Icon name="trash" />删除所选对象</button></div>}
    </aside>
  );
}

function FormSection({ title, children }: { title: string; children: React.ReactNode }) {
  return <section className="form-section"><div className="section-label">{title}</div>{children}</section>;
}

function LineStylePicker({
  label,
  value,
  includeNone = false,
  onChange
}: {
  label: string;
  value: 'solid' | 'dashed' | 'dotted' | 'none';
  includeNone?: boolean;
  onChange: (value: 'solid' | 'dashed' | 'dotted' | 'none') => void;
}) {
  const options: Array<{ value: 'solid' | 'dashed' | 'dotted' | 'none'; label: string }> = [
    { value: 'solid', label: '实线' },
    { value: 'dashed', label: '虚线' },
    { value: 'dotted', label: '点线' },
    ...(includeNone ? [{ value: 'none' as const, label: '无' }] : [])
  ];
  return <div className="style-picker-field"><div className="field-label">{label}</div><div className="line-style-grid">
    {options.map((option) => <button key={option.value} className={value === option.value ? 'selected' : ''} onClick={() => onChange(option.value)} title={option.label} aria-label={`${label}：${option.label}`}>
      <span className={`line-preview ${option.value}`} />
    </button>)}
  </div></div>;
}

function ColorPicker({
  label,
  value,
  colors,
  onChange
}: {
  label: string;
  value?: string;
  colors: string[];
  onChange: (value: string) => void;
}) {
  const pickerValue = /^#[0-9a-f]{6}$/i.test(value ?? '') ? value! : colors[0];
  return <div className="color-picker-field">
    <div className="color-picker-heading"><span className="field-label">{label}</span><label className="custom-color-button" title={`自定义${label}`}>
      <input type="color" value={pickerValue} onChange={(event) => onChange(event.target.value.toUpperCase())} aria-label={`自定义${label}`} />
      <span>自定义</span>
    </label></div>
    <div className="color-grid">
      {colors.map((color) => <button key={color} className={`color-swatch ${value?.toUpperCase() === color ? 'selected' : ''}`} style={{ background: color }} onClick={() => onChange(color)} aria-label={`${label} ${color}`} />)}
    </div>
  </div>;
}
