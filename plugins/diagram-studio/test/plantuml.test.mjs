import assert from 'node:assert/strict';
import test from 'node:test';
import { createTemplate } from '../dist/test-helpers.mjs';
import { detectPlantUmlDiagramKind, diagramToPlantUml, parsePlantUmlActivity, parsePlantUmlSequence, parsePlantUmlStructural, plantUmlToDiagram } from '../dist/plantuml.test.mjs';

test('blank supported diagrams can open PlantUML and round-trip without placeholder nodes', () => {
  for (const kind of ['architecture', 'flowchart', 'swimlane', 'topology', 'sequence']) {
    const template = createTemplate(kind);
    const original = { ...template, title: `空白${kind}`, nodes: [], edges: [] };
    const source = diagramToPlantUml(original);
    assert.match(source, /^@startuml/m);
    assert.match(source, /@diagram-studio-layout/);

    const restored = plantUmlToDiagram(source, { documentId: `${kind}-blank`, kind });
    assert.equal(restored.kind, kind);
    assert.deepEqual(restored.nodes, []);
    assert.deepEqual(restored.edges, []);
  }
});

test('architecture diagrams round-trip through PlantUML Component Diagram with exact canvas state', () => {
  const original = createTemplate('architecture');
  const source = diagramToPlantUml(original);
  assert.match(source, /component "客户端应用" as web_app/);
  assert.match(source, /interface "API Gateway" as api_gateway/);
  assert.match(source, /database "PostgreSQL" as postgres/);
  assert.equal(detectPlantUmlDiagramKind(source), 'architecture');
  assert.equal(detectPlantUmlDiagramKind(source.replace(/^' @diagram-studio-layout .*$/gm, '')), 'architecture');

  const restored = plantUmlToDiagram(source, { documentId: 'architecture-roundtrip' });
  assert.equal(restored.kind, 'architecture');
  assert.deepEqual(restored.nodes, JSON.parse(JSON.stringify(original.nodes)));
  assert.deepEqual(restored.edges, JSON.parse(JSON.stringify(original.edges)));
  assert.equal(restored.notation.dialect, 'component');
});

test('standard PlantUML Component Diagram becomes editable architecture nodes and dependencies', () => {
  const source = `@startuml
title 服务架构
actor "用户" as user
component "Web App" as web
interface "REST API" as api
database "PostgreSQL" as db
queue "Event Bus" as events
user --> web : HTTPS
web --> api : REST
api --> db : SQL
api ..> events : Publish
@enduml`;
  const ir = parsePlantUmlStructural(source);
  assert.equal(ir.nodes.length, 5);
  assert.equal(ir.edges.length, 4);
  assert.equal(detectPlantUmlDiagramKind(source), 'architecture');

  const document = plantUmlToDiagram(source, { documentId: 'component-import' });
  assert.equal(document.kind, 'architecture');
  assert.equal(document.nodes.length, 5);
  assert.equal(document.edges.length, 4);
  assert.ok(document.nodes.some((node) => node.data.icon === 'user'));
  assert.ok(document.nodes.some((node) => node.data.icon === 'database'));
  assert.ok(document.nodes.some((node) => node.data.icon === 'queue'));
  assert.ok(document.edges.some((edge) => edge.data.lineStyle === 'dashed'));
});

test('topology diagrams round-trip through PlantUML Deployment Diagram with exact canvas state', () => {
  const original = createTemplate('topology');
  const source = diagramToPlantUml(original);
  assert.match(source, /cloud "Internet" as internet/);
  assert.match(source, /node "App Node 1" as app_1/);
  assert.match(source, /database "Primary DB" as primary_db/);
  assert.equal(detectPlantUmlDiagramKind(source), 'topology');
  assert.equal(detectPlantUmlDiagramKind(source.replace(/^' @diagram-studio-layout .*$/gm, '')), 'topology');

  const restored = plantUmlToDiagram(source, { documentId: 'topology-roundtrip' });
  assert.equal(restored.kind, 'topology');
  assert.deepEqual(restored.nodes, JSON.parse(JSON.stringify(original.nodes)));
  assert.deepEqual(restored.edges, JSON.parse(JSON.stringify(original.edges)));
  assert.equal(restored.notation.dialect, 'deployment');
});

test('standard PlantUML Deployment Diagram becomes editable topology nodes and links', () => {
  const source = `@startuml
title 生产拓扑
cloud "Internet" as internet
node "Load Balancer" as lb
node "App Node" as app
database "Primary DB" as db
storage "Object Storage" as storage
artifact "Container Image" as image
internet --> lb : HTTPS
lb --> app : HTTP
app --> db : SQL
app ..> storage : Upload
image ..> app : Deploy
@enduml`;
  assert.equal(detectPlantUmlDiagramKind(source), 'topology');
  const document = plantUmlToDiagram(source, { documentId: 'deployment-import' });
  assert.equal(document.kind, 'topology');
  assert.equal(document.nodes.length, 6);
  assert.equal(document.edges.length, 5);
  assert.ok(document.nodes.some((node) => node.data.icon === 'cloud'));
  assert.ok(document.nodes.some((node) => node.data.icon === 'storage'));
  assert.ok(document.nodes.some((node) => node.data.icon === 'document'));
  assert.ok(document.edges.filter((edge) => edge.data.lineStyle === 'dashed').length >= 2);
});

test('Diagram Studio sequence documents round-trip through valid PlantUML without losing canvas interactions', () => {
  const original = createTemplate('sequence');
  const source = diagramToPlantUml(original);
  assert.match(source, /^@startuml/m);
  assert.match(source, /actor "用户" as sequence_user/);
  assert.match(source, /-->/);
  assert.match(source, /@diagram-studio-layout/);

  const restored = plantUmlToDiagram(source, {
    documentId: 'sequence-roundtrip',
    revision: 4,
    createdAt: original.createdAt,
    updatedAt: original.updatedAt
  });
  assert.deepEqual(restored.nodes, JSON.parse(JSON.stringify(original.nodes)));
  assert.deepEqual(restored.edges, JSON.parse(JSON.stringify(original.edges)));
  assert.deepEqual(restored.viewport, original.viewport);
  assert.equal(restored.notation.format, 'plantuml');
  assert.equal(restored.notation.dialect, 'sequence');
});

test('standard PlantUML sequence source becomes editable lifelines, activations, fragments, and horizontal messages', () => {
  const source = `@startuml
title 登录认证
actor "用户" as User
boundary "客户端" as Client
control "认证服务" as Auth
database "数据库" as DB
alt 登录成功
User -> Client: 输入账号
Client -> Auth: 登录请求
activate Auth
Auth -> DB: 查询用户
activate DB
DB --> Auth: 用户数据
deactivate DB
Auth --> Client: Token
deactivate Auth
Client --> User: 展示首页
end
@enduml`;

  const ir = parsePlantUmlSequence(source);
  assert.equal(ir.participants.length, 4);
  assert.equal(ir.messages.length, 6);
  assert.equal(ir.activations.length, 2);
  assert.equal(ir.fragments.length, 1);

  const document = plantUmlToDiagram(source, { documentId: 'sequence-import' });
  assert.equal(document.title, '登录认证');
  assert.equal(document.nodes.filter((node) => node.data.shape === 'lifeline').length, 4);
  assert.equal(document.nodes.filter((node) => node.data.shape === 'activation').length, 2);
  assert.ok(document.nodes.filter((node) => node.data.shape === 'activation').every((node) => node.parentId && node.extent === 'parent'));
  assert.equal(document.nodes.filter((node) => node.data.shape === 'fragment').length, 1);
  assert.equal(document.edges.length, 6);
  assert.ok(document.edges.every((edge) => edge.type === 'straight'));
  assert.ok(document.edges.every((edge) => edge.sourceHandle && edge.targetHandle));
  assert.ok(document.edges.some((edge) => edge.data.lineStyle === 'dashed'));
});

test('editing generated PlantUML rebuilds semantics instead of restoring stale layout metadata', () => {
  const original = createTemplate('sequence');
  const source = diagramToPlantUml(original).replace('1. 发起操作', '1. 打开应用');
  const edited = plantUmlToDiagram(source, { documentId: 'sequence-edited' });
  assert.equal(edited.edges[0].label, '1. 打开应用');
  assert.notDeepEqual(edited.nodes, original.nodes);
});

test('flowcharts round-trip through PlantUML Activity Diagram with decisions and exact canvas layout', () => {
  const original = createTemplate('flowchart');
  const source = diagramToPlantUml(original);
  assert.match(source, /^@startuml/m);
  assert.match(source, /if \(请求有效？\) then \(是\)/);
  assert.match(source, /else \(否\)/);
  assert.equal(detectPlantUmlDiagramKind(source), 'flowchart');

  const restored = plantUmlToDiagram(source, { documentId: 'flow-roundtrip' });
  assert.equal(restored.kind, 'flowchart');
  assert.deepEqual(restored.nodes, JSON.parse(JSON.stringify(original.nodes)));
  assert.deepEqual(restored.edges, JSON.parse(JSON.stringify(original.edges)));
  assert.equal(restored.notation.dialect, 'activity');
});

test('standard Activity Diagram becomes an editable flowchart with labeled decision branches', () => {
  const source = `@startuml
title 订单处理
start
:接收订单;
if (库存充足？) then (是)
  :创建订单;
else (否)
  :返回缺货;
endif
stop
@enduml`;
  const ir = parsePlantUmlActivity(source);
  assert.equal(ir.nodes.length, 6);
  assert.equal(ir.edges.length, 6);
  const document = plantUmlToDiagram(source, { documentId: 'activity-import' });
  assert.equal(document.kind, 'flowchart');
  assert.equal(document.nodes.filter((node) => node.data.shape === 'diamond').length, 1);
  assert.ok(document.edges.some((edge) => edge.label === '是'));
  assert.ok(document.edges.some((edge) => edge.label === '否'));
});

test('PlantUML swimlanes become lane parents and preserve cross-lane activity connections', () => {
  const source = `@startuml
title 发布流程
|产品|
start
:提出需求;
|研发|
:技术设计;
:开发实现;
|测试与发布|
:集成验证;
stop
@enduml`;
  assert.equal(detectPlantUmlDiagramKind(source), 'swimlane');
  const document = plantUmlToDiagram(source, { documentId: 'swimlane-import' });
  assert.equal(document.kind, 'swimlane');
  assert.equal(document.nodes.filter((node) => node.data.shape === 'lane').length, 3);
  const activities = document.nodes.filter((node) => node.data.shape !== 'lane');
  assert.ok(activities.every((node) => node.parentId && node.extent === 'parent'));
  assert.ok(document.edges.length >= 4);

  const roundTripSource = diagramToPlantUml(document);
  assert.match(roundTripSource, /\|产品\|/);
  assert.match(roundTripSource, /\|研发\|/);
  const restored = plantUmlToDiagram(roundTripSource, { documentId: 'swimlane-roundtrip' });
  assert.deepEqual(restored.nodes, JSON.parse(JSON.stringify(document.nodes)));
  assert.deepEqual(restored.edges, JSON.parse(JSON.stringify(document.edges)));
});
