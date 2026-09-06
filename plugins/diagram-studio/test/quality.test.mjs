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
