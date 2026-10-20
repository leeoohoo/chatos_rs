import { describe, expect, it } from "vitest";
import {
  canSubmitProject,
  createdLocalProjectSchema,
  projectDirectorySelectionSchema,
} from "./projectCreation";

describe("project creation domain", () => {
  it("keeps the browser contract opaque and never accepts an absolute path", () => {
    const selection = projectDirectorySelectionSchema.parse({
      selectionId: "folder-selection-1",
      directoryName: "repo",
    });
    expect(selection).toEqual({ selectionId: "folder-selection-1", directoryName: "repo" });
    expect("absolutePath" in selection).toBe(false);
    expect(createdLocalProjectSchema.parse({
      id: "project-1",
      name: "Local",
      revision: 1,
      projectTypeKey: "software_development",
    })).toBeTruthy();
  });

  it("requires a selected folder, valid project type, and nonempty name", () => {
    const base = {
      projectTypes: [{
        key: "software_development",
        label: "通用软件开发",
        description: "",
        categoryKey: "software_product",
        categoryLabel: "软件与数字产品",
      }],
      selection: { selectionId: "selection-1", directoryName: "repo" },
      projectName: " Local ",
      projectTypeKey: "software_development",
      isLoadingTypes: false,
      isPicking: false,
      isSaving: false,
    };
    expect(canSubmitProject(base)).toBe(true);
    expect(canSubmitProject({ ...base, selection: null })).toBe(false);
    expect(canSubmitProject({ ...base, projectName: "   " })).toBe(false);
    expect(canSubmitProject({ ...base, projectTypeKey: "invented" })).toBe(false);
  });
});
