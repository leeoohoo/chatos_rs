import assert from 'node:assert/strict';
import test from 'node:test';
import { layoutDiagram } from '../dist/layout.test.mjs';
import { plantUmlToDiagram } from '../dist/plantuml.test.mjs';
import { inspectDiagramQuality } from '../dist/quality.test.mjs';

test('compound architecture layout keeps a readable overview instead of falling back to one long row', async () => {
  const source = `@startuml
left to right direction
actor "业务用户" as business_user
package "客户端" as client_layer {
  component "Web 管理端" as web_app
}
package "接入层" as entry_layer {
  component "API Gateway" as api_gateway
}
package "业务能力" as domain_layer {
  component "订单域" as order_domain
  component "库存域" as inventory_domain
}
package "数据与基础设施" as data_layer {
  database "业务数据库" as business_db
  queue "可靠任务" as task_queue
}
business_user --> web_app : Uses
web_app --> api_gateway : HTTPS
api_gateway --> order_domain : Routes
api_gateway --> inventory_domain : Routes
order_domain --> inventory_domain : Reserve stock
order_domain --> business_db : SQL
order_domain ..> task_queue : Publish
@enduml`;
  const imported = plantUmlToDiagram(source, { documentId: 'compound-overview', kind: 'architecture' });
  const laidOut = await layoutDiagram(imported, 'RIGHT');
  const report = inspectDiagramQuality(laidOut, 'architecture-overview');
  const topLevelRows = new Set(laidOut.nodes.filter((node) => !node.parentId).map((node) => Math.round(node.position.y)));

  assert.equal(report.valid, true);
  assert.equal(report.ready, true);
  assert.equal(report.metrics.containerCount, 4);
  assert.equal(report.metrics.overlapCount, 0);
  assert.equal(report.metrics.childOverflowCount, 0);
  assert.ok(report.metrics.aspectRatio < 4, `expected a readable aspect ratio, received ${report.metrics.aspectRatio}`);
  assert.ok(topLevelRows.size > 1, 'expected compound boundaries to wrap across more than one row');
});

test('flowchart layout recomputes and distributes decision branch handles after moving nodes', async () => {
  const source = `@startuml
start
:接收回调;
if (回调范围匹配?) then (是)
  if (任务执行成功?) then (是)
    :记录成功结果;
    stop
  else (否)
    :记录失败结果;
    stop
  endif
else (否)
  :拒绝回调并记录审计结果;
  stop
endif
@enduml`;
  const imported = plantUmlToDiagram(source, { documentId: 'flowchart-branch-routing', kind: 'flowchart' });
  for (const edge of imported.edges) {
    edge.sourceHandle = 'right';
    edge.targetHandle = 'left';
  }

  const laidOut = await layoutDiagram(imported, 'DOWN');
  const firstDecision = laidOut.nodes.find((node) => node.data.label === '回调范围匹配?');
  assert.ok(firstDecision);
  const branches = laidOut.edges.filter((edge) => edge.source === firstDecision.id);

  assert.equal(branches.length, 2);
  assert.equal(new Set(branches.map((edge) => edge.sourceHandle)).size, 2, 'decision branches should use separate handle slots');
  assert.equal(branches.some((edge) => edge.sourceHandle === 'bottom'), true);
  assert.equal(branches.some((edge) => edge.sourceHandle === 'left' || edge.sourceHandle === 'right'), true);
  assert.equal(branches.every((edge) => !edge.sourceHandle?.includes('-')), true, 'diamond branches must stay on real vertices');
  assert.equal(branches.every((edge) => edge.targetHandle?.startsWith('top')), true);
});
