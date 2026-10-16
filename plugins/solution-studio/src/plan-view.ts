import type { ExecutionPlan, ExecutionTask } from './schema.js';

// Readiness always uses the canonical task set, never a filtered canvas.
export function readyTaskIds(tasks: readonly ExecutionTask[]): string[] {
  const done = new Set(tasks.filter((task) => task.status === 'done').map((task) => task.id));
  return tasks.filter((task) => task.status === 'planned' && task.dependsOn.every((id) => done.has(id))).map((task) => task.id);
}

export interface DependencyEdge { id: string; source: string; target: string }

export interface DependencyAnalysis {
  topologicalOrder: string[];
  redundantEdges: DependencyEdge[];
  longestChainLength: number;
}

export function dependencyAnalysis(tasks: readonly ExecutionTask[]): DependencyAnalysis {
  const ids = new Set(tasks.map((task) => task.id));
  const taskById = new Map(tasks.map((task) => [task.id, task]));
  const indegree = new Map(tasks.map((task) => [task.id, 0]));
  const outgoing = new Map(tasks.map((task) => [task.id, [] as string[]]));
  for (const task of tasks) {
    for (const dependency of new Set(task.dependsOn)) {
      if (!ids.has(dependency) || dependency === task.id) continue;
      indegree.set(task.id, (indegree.get(task.id) ?? 0) + 1);
      outgoing.get(dependency)?.push(task.id);
    }
  }
  const queue = [...indegree.entries()].filter(([, count]) => count === 0).map(([id]) => id).sort();
  const topologicalOrder: string[] = [];
  const depth = new Map(tasks.map((task) => [task.id, 1]));
  while (queue.length > 0) {
    const id = queue.shift()!;
    topologicalOrder.push(id);
    for (const target of outgoing.get(id) ?? []) {
      depth.set(target, Math.max(depth.get(target) ?? 1, (depth.get(id) ?? 1) + 1));
      const next = (indegree.get(target) ?? 0) - 1;
      indegree.set(target, next);
      if (next === 0) {
        queue.push(target);
        queue.sort();
      }
    }
  }

  const ancestorMemo = new Map<string, Set<string>>();
  const ancestors = (id: string, visiting = new Set<string>()): Set<string> => {
    const cached = ancestorMemo.get(id);
    if (cached) return cached;
    if (visiting.has(id)) return new Set();
    const nextVisiting = new Set(visiting).add(id);
    const result = new Set<string>();
    for (const dependency of taskById.get(id)?.dependsOn ?? []) {
      if (!ids.has(dependency) || dependency === id) continue;
      result.add(dependency);
      for (const ancestor of ancestors(dependency, nextVisiting)) result.add(ancestor);
    }
    ancestorMemo.set(id, result);
    return result;
  };
  const redundantEdges = tasks.flatMap((task) => {
    const dependencies = [...new Set(task.dependsOn)].filter((id) => ids.has(id) && id !== task.id);
    return dependencies.flatMap((source) => dependencies.some((other) => other !== source && ancestors(other).has(source))
      ? [{ id: `${source}->${task.id}`, source, target: task.id }]
      : []);
  });
  return {
    topologicalOrder,
    redundantEdges,
    longestChainLength: topologicalOrder.length === tasks.length
      ? Math.max(0, ...depth.values())
      : 0
  };
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

export function modulePlanView(plan: ExecutionPlan, moduleId: string) {
  const tasks = plan.tasks.filter((task) => task.moduleId === moduleId);
  const ids = new Set(tasks.map((task) => task.id));
  return {
    tasks,
    taskIds: ids,
    readyCount: readyTaskIds(plan.tasks).filter((id) => ids.has(id)).length,
    plan: {
      ...plan,
      tasks,
      positions: Object.fromEntries(Object.entries(plan.positions).filter(([id]) => ids.has(id)))
    }
  };
}

export function visibleDependencyEdges(tasks: readonly ExecutionTask[], includeRedundant = false): DependencyEdge[] {
  const ids = new Set(tasks.map((task) => task.id));
  const edges = tasks.flatMap((task) => [...new Set(task.dependsOn)].filter((id) => ids.has(id)).map((id) => ({ id: `${id}->${task.id}`, source: id, target: task.id })));
  if (includeRedundant) return edges;
  const redundant = new Set(dependencyAnalysis(tasks).redundantEdges.map((edge) => edge.id));
  return edges.filter((edge) => !redundant.has(edge.id));
}
