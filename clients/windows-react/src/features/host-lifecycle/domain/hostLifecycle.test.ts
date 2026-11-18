import { describe, expect, it } from "vitest";
import { machineResourceSnapshotSchema } from "./hostLifecycle";

describe("machineResourceSnapshotSchema", () => {
  it("preserves dynamic Host and App concurrency targets", () => {
    const snapshot = machineResourceSnapshotSchema.parse({
      poolId: "windows-machine-resource-units",
      configuredResourceUnits: 18,
      baselineResourceUnits: 10,
      targetResourceUnits: 14,
      availableResourceUnits: 14,
      maximumBackgroundResourceUnits: 14,
      usedResourceUnits: 6,
      queuedCount: 2,
      pressureLevel: "normal",
      totalLogicalJobs: 10,
      baseHostWorkItems: 5,
      dynamicBorrowingEnabled: true,
      domains: [
        { domain: "mediaWorker", configuredLimit: 4, activeCount: 2, queuedCount: 0 },
        { domain: "codeReadWorker", configuredLimit: 8, activeCount: 4, queuedCount: 2 },
      ],
      host: {
        maximumConcurrentWorkItems: 9,
        targetConcurrentWorkItems: 3,
        effectiveMaximumConcurrentWorkItems: 3,
        pressureLevel: "normal",
        activeWorkItems: 3,
        idleWorkers: 6,
        queuedReadyWorkItems: 0,
        acceptingNewWork: true,
        activeMemorySyncItems: 0,
        admissionInFlightWorkItems: 0,
      },
    });

    const host = snapshot.host!;
    expect(snapshot.targetResourceUnits).toBe(14);
    expect(host.targetConcurrentWorkItems).toBe(3);
    expect(host.targetConcurrentWorkItems + snapshot.targetResourceUnits / 2).toBe(10);
  });
});
