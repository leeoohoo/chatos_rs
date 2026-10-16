import type { ExecutionPlan, ExecutionTask } from './schema.js';

// Readiness always uses the canonical task set, never a filtered canvas.
export function readyTaskIds(tasks: readonly ExecutionTask[]): string[] {
  const done = new Set(tasks.filter((task) => task.status === 'done').map((task) => task.id));
  return tasks.filter((task) => task.status === 'planned' && task.dependsOn.every((id) => done.has(id))).map((task) => task.id);
}

export function requirementPlanView(plan: ExecutionPlan, requirementId: string, scope: 'requirement' | 'complete') {
  const tasks = plan.tasks.filter((task) => task.requirementIds.includes(requirementId));
  const primaryTaskIds = new Set(tasks.map((task) => task.id));
  const taskById = new Map(plan.tasks.map((task) => [task.id, task]));
  const closure = new Set(primaryTaskIds);
  const missingDependencyIds = new Set<string>();
  const queue = tasks.flatMap((task) => task.dependsOn);
  for (let index = 0; index < queue.length; index += 1) {
    const id = queue[index];
    if (closure.has(id)) continue;
    closure.add(id);
    const dependency = taskById.get(id);
    if (dependency) queue.push(...dependency.dependsOn);
    else missingDependencyIds.add(id);
  }
  const supportingTasks = plan.tasks.filter((task) => closure.has(task.id) && !primaryTaskIds.has(task.id));
  const visibleTasks = scope === 'complete' ? plan.tasks.filter((task) => closure.has(task.id)) : tasks;
  const visibleIds = new Set(visibleTasks.map((task) => task.id));
  return {
    tasks,
    primaryTaskIds,
    supportingTasks,
    missingDependencyIds: [...missingDependencyIds],
    readyCount: readyTaskIds(plan.tasks).filter((id) => primaryTaskIds.has(id)).length,
    // Keep the original task objects and dependencies, even when an endpoint is hidden.
    plan: { ...plan, tasks: visibleTasks, positions: Object.fromEntries(Object.entries(plan.positions).filter(([id]) => visibleIds.has(id))) }
  };
}

export function visibleDependencyEdges(tasks: readonly ExecutionTask[]) {
  const ids = new Set(tasks.map((task) => task.id));
  return tasks.flatMap((task) => [...new Set(task.dependsOn)].filter((id) => ids.has(id)).map((id) => ({ id: `${id}->${task.id}`, source: id, target: task.id })));
}
