import type {
  DiagramDocument,
  DiagramEdge,
  DiagramKind,
  DiagramNode,
  DiagramNodeCategory,
  DiagramNodeShape
} from './schema.js';

const palette = {
  blue: '#4E7CC7',
  purple: '#7967D8',
  green: '#4B9B72',
  orange: '#C98145',
  pink: '#B9658D',
  gray: '#667085',
  cyan: '#438FA6'
};

function node(
  id: string,
  label: string,
  x: number,
  y: number,
  category: DiagramNodeCategory,
  shape: DiagramNodeShape = 'rounded',
  subtitle?: string,
  color?: string,
  extra: Partial<DiagramNode> = {},
  dataExtra: Partial<DiagramNode['data']> = {}
): DiagramNode {
  return {
    id,
    type: shape === 'lane' ? 'laneNode' : 'diagramNode',
    position: { x, y },
    data: { label, subtitle, category, shape, color, ...dataExtra },
    ...extra
  };
}

function edge(
  id: string,
  source: string,
  target: string,
  label?: string,
  dashed = false
): DiagramEdge {
  return {
    id,
    source,
    target,
    label,
    type: 'smoothstep',
    data: { relation: label, dashed, lineStyle: dashed ? 'dashed' : 'solid', startMarker: 'none', endMarker: 'arrow', strokeWidth: 1.7 }
  };
}

function sequenceEdge(
  id: string,
  source: string,
  target: string,
  slot: number,
  label: string,
  dashed = false
): DiagramEdge {
  return {
    id,
    source,
    target,
    sourceHandle: `slot-${slot}`,
    targetHandle: `slot-${slot}`,
    label,
    type: 'straight',
    data: { relation: label, dashed, lineStyle: dashed ? 'dashed' : 'solid', startMarker: 'none', endMarker: 'arrow', strokeWidth: 1.7 }
  };
}

function base(kind: DiagramKind, title: string, nodes: DiagramNode[], edges: DiagramEdge[]): DiagramDocument {
  const now = new Date().toISOString();
  return {
    schemaVersion: 1,
    documentId: `${kind}-${crypto.randomUUID().slice(0, 8)}`,
    revision: 0,
    kind,
    title,
    createdAt: now,
    updatedAt: now,
    nodes,
    edges,
    viewport: { x: 0, y: 0, zoom: 1 }
  };
}

export function architectureTemplate(): DiagramDocument {
  return base('architecture', '服务架构图', [
    node('users', '用户', 40, 220, 'client', 'rounded', undefined, palette.purple, {}, { icon: 'user' }),
    node('web-app', '客户端应用', 260, 220, 'client', 'rounded', 'SwiftUI · WinUI', palette.blue, {}, { icon: 'terminal' }),
    node('api-gateway', 'API Gateway', 510, 220, 'network', 'rounded', 'HTTPS · WebSocket', palette.cyan, {}, { icon: 'api' }),
    node('identity', '身份服务', 770, 70, 'service', 'rounded', 'OAuth · Session', palette.purple, {}, { icon: 'shield' }),
    node('project-service', '项目服务', 770, 220, 'service', 'rounded', 'REST · Events', palette.blue, {}, { icon: 'server' }),
    node('task-runner', '任务执行器', 770, 370, 'service', 'rounded', 'AI · MCP', palette.orange, {}, { icon: 'container' }),
    node('postgres', 'PostgreSQL', 1050, 110, 'database', 'rounded', 'Persistent data', palette.green, {}, { icon: 'database' }),
    node('redis', 'Redis', 1050, 260, 'database', 'rounded', 'Cache · Locks', palette.pink, {}, { icon: 'cache' }),
    node('event-bus', '事件总线', 1050, 410, 'queue', 'rounded', 'Async messages', palette.orange, {}, { icon: 'queue' })
  ], [
    edge('e-users-web', 'users', 'web-app'),
    edge('e-web-gateway', 'web-app', 'api-gateway', 'HTTPS'),
    edge('e-gateway-identity', 'api-gateway', 'identity', 'Auth'),
    edge('e-gateway-project', 'api-gateway', 'project-service', 'REST'),
    edge('e-gateway-runner', 'api-gateway', 'task-runner', 'Tasks'),
    edge('e-identity-db', 'identity', 'postgres', 'SQL'),
    edge('e-project-db', 'project-service', 'postgres', 'SQL'),
    edge('e-project-cache', 'project-service', 'redis', 'Cache'),
    edge('e-runner-cache', 'task-runner', 'redis', 'Lease'),
    edge('e-runner-events', 'task-runner', 'event-bus', 'Publish'),
    edge('e-events-project', 'event-bus', 'project-service', 'Consume', true)
  ]);
}

export function flowchartTemplate(): DiagramDocument {
  return base('flowchart', '业务流程图', [
    node('start', '开始', 430, 40, 'terminal', 'circle', undefined, palette.green),
    node('receive', '接收请求', 390, 180, 'process', 'rounded', '校验基础参数', palette.blue),
    node('valid', '请求有效？', 410, 330, 'decision', 'diamond', undefined, palette.orange),
    node('process', '执行业务处理', 390, 510, 'process', 'rounded', '写入状态与事件', palette.purple),
    node('reject', '返回错误', 700, 350, 'process', 'rectangle', '记录拒绝原因', palette.pink),
    node('finish', '完成', 430, 680, 'terminal', 'circle', undefined, palette.green)
  ], [
    edge('e-start-receive', 'start', 'receive'),
    edge('e-receive-valid', 'receive', 'valid'),
    edge('e-valid-process', 'valid', 'process', '是'),
    edge('e-valid-reject', 'valid', 'reject', '否'),
    edge('e-process-finish', 'process', 'finish'),
    edge('e-reject-finish', 'reject', 'finish')
  ]);
}

export function swimlaneTemplate(): DiagramDocument {
  const lanes = [
    node('lane-product', '产品', 30, 30, 'lane', 'lane', undefined, '#EFF3FA', { width: 1120, height: 180, zIndex: 0 }),
    node('lane-engineering', '研发', 30, 230, 'lane', 'lane', undefined, '#F3F0FB', { width: 1120, height: 180, zIndex: 0 }),
    node('lane-qa', '测试与发布', 30, 430, 'lane', 'lane', undefined, '#EEF7F1', { width: 1120, height: 180, zIndex: 0 })
  ];
  const steps = [
    node('requirement', '提出需求', 150, 55, 'process', 'rounded', '定义目标', palette.blue, { parentId: 'lane-product', extent: 'parent', zIndex: 2 }),
    node('review', '需求评审', 410, 55, 'decision', 'diamond', undefined, palette.orange, { parentId: 'lane-product', extent: 'parent', zIndex: 2 }),
    node('design', '技术设计', 230, 55, 'process', 'rounded', '架构与接口', palette.purple, { parentId: 'lane-engineering', extent: 'parent', zIndex: 2 }),
    node('implement', '开发实现', 520, 55, 'process', 'rounded', '代码与测试', palette.blue, { parentId: 'lane-engineering', extent: 'parent', zIndex: 2 }),
    node('verify', '集成验证', 520, 55, 'process', 'rounded', '自动化测试', palette.green, { parentId: 'lane-qa', extent: 'parent', zIndex: 2 }),
    node('release', '发布上线', 800, 55, 'terminal', 'rounded', '灰度与观测', palette.green, { parentId: 'lane-qa', extent: 'parent', zIndex: 2 })
  ];
  return base('swimlane', '泳道流程图', [...lanes, ...steps], [
    edge('e-requirement-review', 'requirement', 'review'),
    edge('e-review-design', 'review', 'design', '通过'),
    edge('e-design-implement', 'design', 'implement'),
    edge('e-implement-verify', 'implement', 'verify'),
    edge('e-verify-release', 'verify', 'release', '通过')
  ]);
}

export function topologyTemplate(): DiagramDocument {
  return base('topology', '基础设施拓扑图', [
    node('internet', 'Internet', 50, 220, 'external', 'rounded', 'Public traffic', palette.purple, {}, { icon: 'cloud' }),
    node('cdn', 'CDN / WAF', 280, 220, 'network', 'rounded', 'Edge protection', palette.cyan, {}, { icon: 'shield' }),
    node('load-balancer', 'Load Balancer', 530, 220, 'network', 'rounded', 'TLS termination', palette.blue, {}, { icon: 'network' }),
    node('app-1', 'App Node 1', 800, 70, 'service', 'rounded', 'Zone A', palette.green, {}, { icon: 'server' }),
    node('app-2', 'App Node 2', 800, 220, 'service', 'rounded', 'Zone B', palette.green, {}, { icon: 'server' }),
    node('app-3', 'App Node 3', 800, 370, 'service', 'rounded', 'Zone C', palette.green, {}, { icon: 'server' }),
    node('primary-db', 'Primary DB', 1080, 140, 'database', 'rounded', 'Writer', palette.orange, {}, { icon: 'database' }),
    node('replica-db', 'Read Replica', 1080, 310, 'database', 'rounded', 'Reader', palette.pink, {}, { icon: 'database' }),
    node('monitoring', 'Monitoring', 790, 540, 'external', 'rounded', 'Metrics · Logs', palette.gray, {}, { icon: 'monitor' })
  ], [
    edge('e-internet-cdn', 'internet', 'cdn', 'HTTPS'),
    edge('e-cdn-lb', 'cdn', 'load-balancer', 'HTTPS'),
    edge('e-lb-app1', 'load-balancer', 'app-1'),
    edge('e-lb-app2', 'load-balancer', 'app-2'),
    edge('e-lb-app3', 'load-balancer', 'app-3'),
    edge('e-app1-primary', 'app-1', 'primary-db', 'SQL'),
    edge('e-app2-primary', 'app-2', 'primary-db', 'SQL'),
    edge('e-app3-replica', 'app-3', 'replica-db', 'Read'),
    edge('e-primary-replica', 'primary-db', 'replica-db', 'Replication', true),
    edge('e-app1-monitor', 'app-1', 'monitoring', 'Metrics', true),
    edge('e-app2-monitor', 'app-2', 'monitoring', 'Metrics', true),
    edge('e-app3-monitor', 'app-3', 'monitoring', 'Metrics', true)
  ]);
}

export function sequenceTemplate(): DiagramDocument {
  const lifelines = [
    node('sequence-user', '用户', 30, 30, 'client', 'lifeline', undefined, palette.purple, { width: 160, height: 620, zIndex: 1 }, { icon: 'user', showLabel: true }),
    node('sequence-client', '客户端', 270, 30, 'client', 'lifeline', undefined, palette.blue, { width: 160, height: 620, zIndex: 1 }, { icon: 'terminal', showLabel: true }),
    node('sequence-service', '业务服务', 510, 30, 'service', 'lifeline', undefined, palette.purple, { width: 160, height: 620, zIndex: 1 }, { icon: 'server', showLabel: true }),
    node('sequence-db', '数据库', 750, 30, 'database', 'lifeline', undefined, palette.green, { width: 160, height: 620, zIndex: 1 }, { icon: 'database', showLabel: true })
  ];
  const activations = [
    node('activation-client', '客户端激活', 343, 145, 'process', 'activation', undefined, palette.blue, { width: 14, height: 330, zIndex: 3 }, { showLabel: false, fillColor: '#E8F1FF' }),
    node('activation-service', '服务激活', 583, 205, 'process', 'activation', undefined, palette.purple, { width: 14, height: 220, zIndex: 3 }, { showLabel: false, fillColor: '#EEEAFE' }),
    node('activation-db', '数据库激活', 823, 265, 'process', 'activation', undefined, palette.green, { width: 14, height: 100, zIndex: 3 }, { showLabel: false, fillColor: '#E8F6ED' })
  ];
  return base('sequence', '用户操作时序', [...lifelines, ...activations], [
    sequenceEdge('seq-e1', 'sequence-user', 'sequence-client', 6, '1. 发起操作'),
    sequenceEdge('seq-e2', 'sequence-client', 'sequence-service', 11, '2. API 请求'),
    sequenceEdge('seq-e3', 'sequence-service', 'sequence-db', 16, '3. 查询'),
    sequenceEdge('seq-e4', 'sequence-db', 'sequence-service', 21, '4. 数据结果', true),
    sequenceEdge('seq-e5', 'sequence-service', 'sequence-client', 26, '5. 响应', true),
    sequenceEdge('seq-e6', 'sequence-client', 'sequence-user', 31, '6. 展示结果', true)
  ]);
}

export function createTemplate(kind: DiagramKind): DiagramDocument {
  switch (kind) {
    case 'architecture': return architectureTemplate();
    case 'flowchart': return flowchartTemplate();
    case 'swimlane': return swimlaneTemplate();
    case 'topology': return topologyTemplate();
    case 'sequence': return sequenceTemplate();
  }
}

export function createBlankDiagram(kind: DiagramKind, title: string): DiagramDocument {
  return base(kind, title.trim().slice(0, 240), [], []);
}

export const diagramTypeCatalog: Array<{ kind: DiagramKind; title: string; subtitle: string }> = [
  { kind: 'architecture', title: '架构图', subtitle: '服务、数据和调用关系' },
  { kind: 'flowchart', title: '流程图', subtitle: '步骤、判断和分支' },
  { kind: 'swimlane', title: '泳道图', subtitle: '角色、阶段和责任边界' },
  { kind: 'topology', title: '拓扑图', subtitle: '节点、网络和基础设施' },
  { kind: 'sequence', title: '时序图', subtitle: '参与者、调用顺序和消息返回' }
];
