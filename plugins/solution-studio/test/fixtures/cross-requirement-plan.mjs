export function crossRequirementPlan() {
  const task = (id, requirementIds, dependsOn) => ({ id, moduleId: 'MOD-CORE', title: id, requirementIds, dependsOn, status: 'planned', type: 'task', phase: '实现', description: '', designSectionIds: [], deliverables: ['产物'], acceptanceCriteria: ['可验收'], sourceReferences: [] });
  return { status: 'draft', revision: 1, basedOnDesignRevision: 0, objective: '', statusEvents: [], positions: {}, viewport: { x: 0, y: 0, zoom: 1 }, updatedAt: new Date().toISOString(), tasks: [
    task('T-001', ['R-001', 'R-002', 'R-003'], []),
    task('T-002', ['R-001'], ['T-001']),
    task('T-003', ['R-002'], ['T-002']),
    task('T-004', ['R-003'], ['T-001', 'T-003']),
    task('T-005', ['R-001', 'R-002', 'R-003'], ['T-004'])
  ] };
}
