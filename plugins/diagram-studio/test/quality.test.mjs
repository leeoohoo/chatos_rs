import assert from 'node:assert/strict';
import test from 'node:test';
import { inspectDiagramQuality } from '../dist/quality.test.mjs';
import { plantUmlToDiagram } from '../dist/plantuml.test.mjs';

test('architecture overview quality blocks dense dependency graphs', () => {
  const components = Array.from({ length: 14 }, (_, index) => `component "Service ${index + 1}" as service_${index + 1}`).join('\n');
  const edges = Array.from({ length: 13 }, (_, index) => `service_1 --> service_${index + 2}`).join('\n');
  const document = plantUmlToDiagram(`@startuml\npackage "Services" as services {\n${components}\n}\n${edges}\n@enduml`, {
    documentId: 'dense-overview',
    kind: 'architecture'
  });
  const report = inspectDiagramQuality(document, 'architecture-overview');
  assert.equal(report.valid, true);
  assert.equal(report.ready, false);
  assert.ok(report.warnings.some((warning) => warning.code === 'architecture_too_many_components' && warning.blocking));
  assert.ok(report.warnings.some((warning) => warning.code === 'architecture_hub_overloaded' && warning.blocking));
});

test('source evidence can be required for delivery readiness', () => {
  const document = plantUmlToDiagram('@startuml\ncomponent "Web" as web\ncomponent "API" as api\nweb --> api : HTTPS\n@enduml', {
    documentId: 'evidence-required',
    kind: 'architecture'
  });
  const advisory = inspectDiagramQuality(document, 'balanced', false);
  const required = inspectDiagramQuality(document, 'balanced', true);
  assert.equal(advisory.ready, true);
  assert.equal(required.ready, false);
  assert.ok(required.warnings.some((warning) => warning.code === 'missing_source_references' && warning.blocking));
});

test('architecture overview quality blocks runtime cycles and cross-boundary edge stars', () => {
  const document = plantUmlToDiagram(`@startuml
left to right direction
actor "User" as user
package "Client" as client_boundary { component "Client App" as client }
package "Core" as core_boundary { component "ChatOS Core" as core }
package "Task" as task_boundary { component "Task Runner" as task }
package "Capability" as capability_boundary { component "Tool Runtime" as tools }
package "Data" as data_boundary {
  database "State" as state
  component "Memory" as memory
  queue "Events" as events
}
user --> client : Uses
client --> core : HTTPS
core --> task : Submit work
task ..> core : Callback result
core --> tools : Invoke tools
task --> tools : Runtime tools
core --> state : Persist messages
core --> memory : Read context
core ..> events : Consume results
task --> state : Persist runs
task --> memory : Sync project
task ..> events : Publish work
@enduml`, { documentId: 'runtime-cycle-overview', kind: 'architecture' });
  const report = inspectDiagramQuality(document, 'architecture-overview');
  const codes = new Set(report.warnings.filter((warning) => warning.blocking).map((warning) => warning.code));

  assert.equal(report.ready, false);
  assert.ok(codes.has('architecture_reciprocal_relationships'));
  assert.ok(codes.has('architecture_cross_boundary_hub'));
  assert.ok(codes.has('architecture_boundary_pair_too_dense'));
  assert.equal(report.metrics.reciprocalRelationshipCount, 1);
  assert.ok(report.metrics.maxCrossBoundaryFan > 4);
  assert.ok(report.metrics.maxBoundaryPairEdges > 2);
});
