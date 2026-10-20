import { describe, expect, it } from "vitest";
import { projectRunAnalysis, runningProjectState } from "../../../test/projectRunFixture";
import {
  projectRunAnalysisSnapshotSchema,
  projectRunStateSchema,
  requiredProjectRunToolchains,
  selectedProjectRunTarget,
} from "./projectRun";

describe("project run domain", () => {
  it("parses the native safe projection and derives the selected target", () => {
    const snapshot = projectRunAnalysisSnapshotSchema.parse(projectRunAnalysis());

    expect(selectedProjectRunTarget(snapshot.catalog)?.id).toBe("target-1");
    expect(requiredProjectRunToolchains(snapshot.catalog)).toEqual(["node", "npm"]);
    expect(JSON.stringify(snapshot)).not.toContain("C:\\\\");
  });

  it("rejects absolute or parent-traversing paths from the native projection", () => {
    const absolute = projectRunAnalysis();
    absolute.catalog.targets[0].cwd = "C:\\Users\\owner\\project";
    expect(() => projectRunAnalysisSnapshotSchema.parse(absolute)).toThrow();

    const traversal = projectRunAnalysis();
    traversal.environment.configurationFiles[0].path = "../secret.env";
    expect(() => projectRunAnalysisSnapshotSchema.parse(traversal)).toThrow();
  });

  it("rejects inconsistent process state", () => {
    const state = runningProjectState({ isRunning: false, status: "idle" });
    expect(() => projectRunStateSchema.parse(state)).toThrow(/项目运行状态与实例不一致/);
  });
});
